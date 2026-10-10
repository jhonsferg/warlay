import { defineStore } from "pinia";
import { ref } from "vue";
import {
  createRoom as apiCreateRoom,
  deleteRoom as apiDeleteRoom,
  joinRoom as apiJoinRoom,
  launchGame,
  leaveRoom as apiLeaveRoom,
  setRoomAvailable as apiSetRoomAvailable,
  setRoomInProgress as apiSetRoomInProgress,
  startHostingTunnel,
  startJoiningTunnel,
  stopHubTunnel,
  toggleMutePeer as apiToggleMutePeer,
} from "../api";
import type { RoomMember, RoomSummary } from "../types";
import { DEFAULT_HUB_TCP_TUNNEL_PORT, DEFAULT_HUB_TLS_TUNNEL_PORT, parseServerAddress } from "../types";
import { playStartGame } from "../utils/audio";
import { buildLaunchArgs } from "../utils/launch";
import { useChatStore } from "./chat";
import { useServersStore } from "./servers";
import { useSettingsStore } from "./settings";
import { useUiStore } from "./ui";

export interface ActiveRoom {
  room: RoomSummary;
  relayToken: string;
  isHost: boolean;
  /** Which server's connection this room lives on — only one real game
   * session can be active at a time regardless of how many servers are
   * connected, but this still needs to be tagged so e.g. `DangerZone` can
   * tell whether deleting a *different* server is safe. */
  serverId: string;
  /** True once this room's WC3 process has exited — exiting no longer
   * leaves the room (the hub already treats `LeaveRoom` as non-destructive,
   * but there's no reason to give up this client's room membership just
   * because the local game window closed). Drives the "Reanudar partida"
   * button; reset to `false` whenever a fresh `roomCreated`/`joinedRoom`
   * replaces this object, or by `resumeGame`. */
  gameExited: boolean;
  /** Whether the daemon actually has a live tunnel/LAN mesh open for this
   * room right now. Starts `false` on every fresh `roomCreated`/`joinedRoom`
   * — never set directly by this store's own calls, only synced from the
   * daemon's own `TunnelStatusChanged` push (`onHubTunnelStatusChanged` in
   * `bootstrap.ts`), since that's the one signal that reflects reality
   * (including a daemon restart mid-session) rather than just "our last
   * request succeeded". */
  tunnelActive: boolean;
  /** Real members of the active room pushed by the hub via RoomMembersChanged. */
  members: RoomMember[];
}

export const useRoomsStore = defineStore("rooms", () => {
  const serversStore = useServersStore();
  const settingsStore = useSettingsStore();
  const uiStore = useUiStore();

  const activeRoom = ref<ActiveRoom | null>(null);
  const activityLog = ref<string[]>([]);
  /** Guards against a real race: the launch button can call
   * `setupTunnelForActiveRoom` while it's still in flight (`startHostingTunnel`
   * takes a moment to actually connect), and the game-running watcher in
   * `bootstrap.ts` can fire concurrently once the optimistic launch event
   * lands — without this, both paths can call `StartHostingTunnel` at once. */
  const tunnelSetupInFlight = ref(false);

  const mutedClientIds = ref<Set<string>>(new Set());
  const speakingTimestamps = ref<Map<string, number>>(new Map());

  function toggleMutePeer(clientId: string): void {
    void apiToggleMutePeer(clientId);
  }

  function updateMutedPeers(clientIds: string[]): void {
    mutedClientIds.value = new Set(clientIds);
  }

  function isPeerMuted(clientId: string): boolean {
    return mutedClientIds.value.has(clientId);
  }

  function recordPeerSpeaking(clientId: string): void {
    speakingTimestamps.value.set(clientId, Date.now());
  }

  function isPeerSpeaking(clientId: string): boolean {
    const ts = speakingTimestamps.value.get(clientId);
    if (!ts) return false;
    return Date.now() - ts < 400;
  }

  function logActivity(message: string): void {
    activityLog.value.push(message);
  }

  async function createRoom(serverId: string, name: string, gamePort: number, password?: string): Promise<void> {
    const connection = serversStore.serverConnections.get(serverId);
    if (connection) connection.errorMessage = null;
    await apiCreateRoom(serverId, name, gamePort, password);
  }

  async function joinRoom(serverId: string, roomId: string, password?: string): Promise<void> {
    const connection = serversStore.serverConnections.get(serverId);
    if (connection) connection.errorMessage = null;
    await apiJoinRoom(serverId, roomId, password || undefined);
  }

  async function leaveRoom(): Promise<void> {
    if (!activeRoom.value) return;
    await apiLeaveRoom(activeRoom.value.serverId);
    await stopHubTunnel();
  }

  /** Permanently closes a room for everyone — the hub only allows this for
   * the room's original creator. See `RoomSummary.hostClientId`. */
  async function deleteRoom(serverId: string, roomId: string): Promise<void> {
    await apiDeleteRoom(serverId, roomId);
    if (activeRoom.value?.room.id === roomId) {
      await stopHubTunnel();
    }
  }

  /** Host-only: reopens a closed room so people can join again ("Abrir
   * sala" -- the other half of `closeRoomToNewJoiners`). Also reached two
   * other ways: automatically, driven by the daemon observing W3GS bytes
   * on the host's tunnel/LAN traffic (`daemon/src/main.rs`'s
   * `game_started` flag + `contains_create_game`) when the host re-hosts a
   * new game; and from `bootstrap.ts`'s `handleGameExited`, when the
   * host's WC3 exits for real instead of re-hosting (a re-host that never
   * comes would otherwise leave the room stuck "En partida" forever).
   * Calling this when the room's already available is a harmless no-op on
   * the hub side. */
  async function markRoomAvailable(): Promise<void> {
    if (!activeRoom.value) return;
    await apiSetRoomAvailable(activeRoom.value.serverId);
  }

  /** Host-only: locks the room so nobody new can join, like closing a
   * door — independent of whether a game is actually running. Exists
   * because the automatic detection (byte-sniffing the host's own W3GS
   * traffic) is best-effort and has real gaps (a joiner is required for
   * `COUNTDOWN_START` to even be observable at all, see
   * `daemon/src/handlers.rs`'s `StartHostingTunnel` docs) — this gives the
   * host a reliable, explicit alternative: gather everyone expected, close
   * the room, and reopen it later (`markRoomAvailable`) if someone still
   * needs to get in. Reuses the same `RoomStatus`/join-gating machinery
   * `RoomItem.vue` already has for the automatic path (`room.status ===
   * "inProgress"` disables joining) — both can coexist, this is just
   * another way to trigger the same state. */
  async function closeRoomToNewJoiners(): Promise<void> {
    if (!activeRoom.value) return;
    await apiSetRoomInProgress(activeRoom.value.serverId);
  }

  /** Just the "start the WC3 process" half — shared by `startGame` and
   * `resumeGame`, neither of which need to redo tunnel setup (the
   * tunnel/mesh is already alive and survives a WC3 restart). Exposed from
   * this store (rather than kept private) so callers outside it can share
   * the same `activityLog`.
   */
  async function launchConfiguredGame_(): Promise<void> {
    if (!settingsStore.settings.gameExecutablePath.trim()) {
      logActivity("Inícialo manualmente — no hay ejecutable configurado.");
      return;
    }

    try {
      logActivity("Lanzando el juego…");
      const args = buildLaunchArgs(settingsStore.settings);
      const result = await launchGame(settingsStore.settings.gameExecutablePath, args, null);
      logActivity(`Juego lanzado (PID ${result.pid}).`);
    } catch (error) {
      logActivity(`No se pudo lanzar el juego: ${String(error)}`);
    }
  }

  /** Opens the daemon-side tunnel/LAN mesh for whichever room is currently
   * active, without touching the game process. Used both for the "hot-swap"
   * case (game already running, room just changed — see `bootstrap.ts`'s
   * `maybeHotSwapTunnel`) and as the first half of `startGame` when nothing
   * has plumbed a tunnel yet. Safe to call redundantly: no-ops if there's no
   * active room, one is already in flight, or `tunnelActive` is already
   * true for this exact room object. */
  async function setupTunnelForActiveRoom(): Promise<void> {
    const room = activeRoom.value;
    if (!room || tunnelSetupInFlight.value || room.tunnelActive) return;

    const server = serversStore.servers.get(room.serverId);
    if (!server) {
      logActivity("No se pudo determinar el servidor activo.");
      return;
    }
    const endpoint = parseServerAddress(server.address);
    if (!endpoint) {
      logActivity("La dirección del servidor no es válida.");
      return;
    }
    const { host: hubHost, secure } = endpoint;
    const hubTunnelPort = secure ? DEFAULT_HUB_TLS_TUNNEL_PORT : DEFAULT_HUB_TCP_TUNNEL_PORT;

    tunnelSetupInFlight.value = true;
    try {
      if (room.isHost) {
        logActivity("Abriendo el túnel de la sesión de juego hacia el servidor…");
        await startHostingTunnel(hubHost, hubTunnelPort, room.relayToken, room.room.gamePort, secure);
        logActivity(
          'Túnel activo. Al crear tu partida en "Red de Área Local" dentro del juego, se retransmitirá tal cual a los demás miembros de la sala.',
        );
      } else {
        logActivity("Abriendo el túnel hacia el servidor y esperando la partida real del anfitrión…");
        await startJoiningTunnel(hubHost, hubTunnelPort, room.relayToken, room.room.gamePort, secure);
        logActivity('Listo — cuando el anfitrión cree su partida, debería aparecer en tu "Red de Área Local".');
      }
      // Set directly here rather than waiting for `onHubTunnelStatusChanged`
      // to catch up: that event and this call's own success both come from
      // the same daemon action, but travel to the frontend over different
      // Tauri IPC paths (a broadcast event vs. this command's return value)
      // with no guaranteed relative ordering between them. Without this, a
      // slow tunnel connect can let the game-running watcher's hot-swap
      // backstop (see bootstrap.ts) fire again before the event lands, still
      // seeing `tunnelActive: false` and sending a second, redundant
      // StartHostingTunnel/StartJoiningTunnel call — confirmed via a real
      // double-call in the daemon log during a slow (~15s) VPS connect. The
      // event listener stays wired for the cases this call's own success
      // can't cover: a daemon restart resync, or the tunnel dying later.
      if (activeRoom.value === room) {
        room.tunnelActive = true;
      }
      playStartGame();
      const chatStore = useChatStore();
      void chatStore.notifyUser(
        "Warlay - Partida Lista",
        `La sala "${room.room.name}" está lista. El túnel ha sido abierto.`,
      );
    } catch (error) {
      logActivity(`No se pudo iniciar el túnel: ${String(error)}`);
    } finally {
      tunnelSetupInFlight.value = false;
    }
  }

  /** The "Iniciar Frozen Throne" entry point: makes sure the active room (if
   * any) actually has a tunnel before launching, then launches. If there's
   * no active room this just launches directly, same as the standalone LAN
   * tab's own launch button. */
  async function startGame(): Promise<void> {
    if (activeRoom.value && !activeRoom.value.tunnelActive) {
      await setupTunnelForActiveRoom();
    }
    await launchConfiguredGame_();
  }

  /** Relaunches WC3 for the room the player is still in after it exited —
   * exiting no longer leaves the room (see `bootstrap.ts`'s
   * `handleGameExited`). The tunnel is still alive (`tunnelActive` wasn't
   * touched by the exit), so `startGame` skips straight to relaunching.
   */
  async function resumeGame(): Promise<void> {
    if (!activeRoom.value) return;
    activeRoom.value.gameExited = false;
    uiStore.showExitBanner = false;
    await startGame();
  }

  return {
    activeRoom,
    activityLog,
    tunnelSetupInFlight,
    mutedClientIds,
    speakingTimestamps,
    toggleMutePeer,
    updateMutedPeers,
    isPeerMuted,
    recordPeerSpeaking,
    isPeerSpeaking,
    logActivity,
    createRoom,
    joinRoom,
    leaveRoom,
    deleteRoom,
    markRoomAvailable,
    closeRoomToNewJoiners,
    launchConfiguredGame_,
    setupTunnelForActiveRoom,
    startGame,
    resumeGame,
  };
});
