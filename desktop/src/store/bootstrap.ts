import { watch } from "vue";
import {
  getClientId,
  getMutedPeers,
  isGameRunning,
  listLocalSubnets,
  listServers,
  loadSettings,
  onDiscoveryCompleted,
  onGameExited,
  onGameRunningStateChanged,
  onGamesUpdated,
  onHubTunnelStatusChanged,
  onLogLine,
  onMatchmakingMessage,
  onMatchmakingPing,
  onMatchmakingStatus,
  onMuteChanged,
  onRelayFound,
  onRelayStatusChanged,
  onVoiceFrameIn,
  pushVoiceFrame,
  startVoiceChat,
  stopVoiceChat,
  stopHubTunnel,
  stopWatchGames,
} from "../api";
import type { RoomSummary, ServerInfo, ServerMessage } from "../types";
import { useConsoleStore } from "./console";
import { useGameProcessStore } from "./gameProcess";
import { useLanMeshStore } from "./lanMesh";
import { useRoomsStore } from "./rooms";
import { freshConnectionState, useServersStore } from "./servers";
import { useSettingsStore } from "./settings";
import { useUiStore } from "./ui";
import { useChatStore } from "./chat";

/** The genuinely cross-domain glue that doesn't belong to any one store —
 * these three functions each touch servers + rooms (+ sometimes LAN-tunnel
 * / activity log) at once, and `bootstrap()` wires up every backend event
 * listener plus initial state load. This module owns no state of its own;
 * it only orchestrates calls into the domain stores. `App.vue` calls
 * `bootstrap()` once from `onMounted`, same as it always has. */

/** Called right after a room becomes active. Used to decide whether to
 * plumb a tunnel immediately (the game is already running — this is the
 * "hot-swap" case: the user is switching rooms without closing WC3) or to
 * leave it for later (nothing to bridge to yet — the tunnel opens only once
 * the user actually starts the game, via `roomsStore.startGame()`). Never
 * launches the game itself — joining/creating a room no longer implies
 * that on its own. */
function maybeHotSwapTunnel(room: RoomSummary): void {
  const roomsStore = useRoomsStore();
  const gameProcessStore = useGameProcessStore();

  roomsStore.activityLog = [];
  roomsStore.logActivity(`Sala "${room.name}" lista (puerto ${room.gamePort}).`);

  if (gameProcessStore.running) {
    void roomsStore.setupTunnelForActiveRoom();
  } else {
    roomsStore.logActivity("El túnel se abrirá cuando inicies el juego.");
  }
}

function handleServerMessage(serverId: string, message: ServerMessage): void {
  const serversStore = useServersStore();
  const roomsStore = useRoomsStore();
  const settingsStore = useSettingsStore();

  const connection = serversStore.serverConnections.get(serverId);
  if (!connection) return;
  switch (message.type) {
    case "authenticated":
      connection.authenticated = true;
      break;
    case "roomList":
      connection.rooms = new Map(message.rooms.map((room) => [room.id, room]));
      break;
    case "roomCreated":
      connection.rooms.set(message.room.id, message.room);
      roomsStore.activeRoom = {
        room: message.room,
        relayToken: message.relayToken,
        isHost: true,
        serverId,
        gameExited: false,
        tunnelActive: false,
        members: [],
      };
      maybeHotSwapTunnel(message.room);
      if (settingsStore.settings.voiceEnabled) {
        void startVoiceChat(
          message.relayToken,
          settingsStore.settings.microphoneDeviceId || null,
          settingsStore.settings.audioOutputDeviceId || null,
        );
      }
      void getMutedPeers()
        .then((muted) => roomsStore.updateMutedPeers(muted))
        .catch(() => {});
      break;
    case "roomUpdated":
      connection.rooms.set(message.room.id, message.room);
      if (roomsStore.activeRoom && roomsStore.activeRoom.room.id === message.room.id) {
        roomsStore.activeRoom.room = message.room;
      }
      break;
    case "roomRemoved":
      connection.rooms.delete(message.roomId);
      if (roomsStore.activeRoom?.room.id === message.roomId) {
        roomsStore.activeRoom = null;
        void stopHubTunnel();
        void stopVoiceChat();
      }
      break;
    case "joinedRoom":
      connection.rooms.set(message.room.id, message.room);
      roomsStore.activeRoom = {
        room: message.room,
        relayToken: message.relayToken,
        isHost: false,
        serverId,
        gameExited: false,
        tunnelActive: false,
        members: [],
      };
      maybeHotSwapTunnel(message.room);
      if (settingsStore.settings.voiceEnabled) {
        void startVoiceChat(
          message.relayToken,
          settingsStore.settings.microphoneDeviceId || null,
          settingsStore.settings.audioOutputDeviceId || null,
        );
      }
      void getMutedPeers()
        .then((muted) => roomsStore.updateMutedPeers(muted))
        .catch(() => {});
      break;
    case "leftRoom":
      roomsStore.activeRoom = null;
      void stopVoiceChat();
      break;
    case "roomMembersChanged":
      if (roomsStore.activeRoom && roomsStore.activeRoom.room.id === message.roomId) {
        roomsStore.activeRoom.members = message.members;
      }
      break;
    case "serverStats":
      connection.connectedClients = message.connectedClients;
      break;
    case "chatMessage": {
      connection.chatMessages.push({
        fromDisplayName: message.fromDisplayName,
        fromClientId: message.fromClientId,
        text: message.text,
        unixMillis: message.unixMillis,
      });
      const chatLimit = settingsStore.settings.chatMessageLimit || 500;
      if (connection.chatMessages.length > chatLimit) {
        connection.chatMessages.splice(0, connection.chatMessages.length - chatLimit);
      }
      break;
    }
    case "privateMessage": {
      const chatStore = useChatStore();
      const entry = {
        fromDisplayName: message.fromDisplayName,
        fromClientId: message.fromClientId,
        text: message.text,
        unixMillis: message.unixMillis,
      };

      const otherId = message.fromClientId === settingsStore.clientId ? message.toClientId : message.fromClientId;
      chatStore.handleIncomingPrivateMessage(entry, otherId, settingsStore.clientId);
      break;
    }
    case "error":
      connection.errorMessage = message.message;
      break;
    case "pong":
      break;
  }
}

async function handleGameExited(): Promise<void> {
  const roomsStore = useRoomsStore();
  const uiStore = useUiStore();

  // Doesn't leave the room — the shared room status (En Partida ↔ Sala
  // abierta) is primarily driven by the daemon's W3GS byte hooks (see
  // daemon/src/main.rs's game_started flag): the host re-announcing a new
  // game is the only *reliable* signal the previous session ended. But
  // that signal never fires if the host just quits WC3 entirely instead of
  // re-hosting -- the room would stay "En partida" forever with nobody
  // able to correct it. This is the fallback for exactly that: the host's
  // own WC3 genuinely exiting (this event only fires past
  // launcher.rs's MIN_REAL_SESSION, so a launcher-stub handoff never
  // triggers a false "available") is just as valid a "the match is over"
  // signal, so mark the room available directly rather than waiting for a
  // re-host that may never come. Only the host does this -- matches the
  // hub's own model where only the host's actions ever change the room's
  // shared status.
  if (roomsStore.activeRoom) {
    roomsStore.activeRoom.gameExited = true;
    if (roomsStore.activeRoom.isHost) {
      void roomsStore.markRoomAvailable();
    }
  }
  uiStore.showExitBanner = true;
}

export async function bootstrap(): Promise<void> {
  const serversStore = useServersStore();
  const uiStore = useUiStore();
  const settingsStore = useSettingsStore();
  const consoleStore = useConsoleStore();
  const lanMeshStore = useLanMeshStore();

  await onRelayFound((relay) => {
    lanMeshStore.relays.set(relay.addr, relay);
  });
  await onDiscoveryCompleted(() => {});
  await onGamesUpdated((games) => {
    lanMeshStore.games = games;
  });
  await onRelayStatusChanged((status) => {
    lanMeshStore.tunnelRunning = status.running;
  });

  await onMatchmakingStatus(({ serverId, connected }) => {
    const connection = serversStore.serverConnections.get(serverId);
    if (!connection || connected) return;
    connection.status = "disconnected";
    connection.authenticated = false;
    connection.pingMs = null;
    connection.connectedClients = null;
  });
  await onMatchmakingPing(({ serverId, pingMs }) => {
    const connection = serversStore.serverConnections.get(serverId);
    if (connection) connection.pingMs = pingMs;
  });
  await onMatchmakingMessage(({ serverId, message }) => handleServerMessage(serverId, message));
  await onHubTunnelStatusChanged((running) => {
    // The one real signal for "is the active room's tunnel actually up" —
    // reflects the daemon's own `game_tunnel` state, including a resync on
    // daemon restart, not just whatever this client last assumed after its
    // own `StartHostingTunnel`/`StartJoiningTunnel` call succeeded.
    const roomsStore = useRoomsStore();
    if (roomsStore.activeRoom) {
      roomsStore.activeRoom.tunnelActive = running;
    }
  });
  await onGameRunningStateChanged((running) => {
    useGameProcessStore().running = running;
  });
  await onGameExited(() => void handleGameExited());
  await onLogLine((line) => {
    consoleStore.pushLine(line);
  });
  await onVoiceFrameIn(({ fromClientId, opusBytes }) => {
    const roomsStore = useRoomsStore();
    roomsStore.recordPeerSpeaking(fromClientId);
    void pushVoiceFrame(fromClientId, opusBytes);
  });
  await onMuteChanged((mutedClientIds) => {
    useRoomsStore().updateMutedPeers(mutedClientIds);
  });

  // Backstop for Warcraft III started outside the new "Iniciar Frozen
  // Throne" button (e.g. launched manually) while a room is already active
  // but has no tunnel yet — the button's own click already calls
  // `setupTunnelForActiveRoom` directly for the common case; this catches
  // the transition when nothing we did caused it.
  {
    const roomsStore = useRoomsStore();
    const gameProcessStore = useGameProcessStore();
    watch(
      () => gameProcessStore.running,
      (running, wasRunning) => {
        if (running && !wasRunning && roomsStore.activeRoom && !roomsStore.activeRoom.tunnelActive) {
          void roomsStore.setupTunnelForActiveRoom();
        }
      },
    );
  }

  try {
    settingsStore.settings = await loadSettings();
  } catch {
    /* first run: keep the defaults */
  }

  try {
    settingsStore.clientId = await getClientId();
  } catch {
    settingsStore.clientId = "";
  }

  let servers: ServerInfo[] = [];
  try {
    servers = await listServers();
    serversStore.servers = new Map(servers.map((server) => [server.id, server]));
  } catch {
    /* leave the list empty; EmptyState surfaces the zero-servers case */
  }

  for (const server of servers) {
    serversStore.serverConnections.set(server.id, freshConnectionState());
  }
  if (servers.length > 0) {
    uiStore.railSelection = { kind: "server", serverId: servers[0].id };
  }

  // Fire-and-forget: every registered server connects in the background
  // (Discord-style), each showing "conectando…" independently rather than
  // blocking the rest of bootstrap on all of them finishing.
  void Promise.allSettled(servers.map((server) => serversStore.connectServer(server.id)));

  try {
    lanMeshStore.subnets = await listLocalSubnets();
    lanMeshStore.selectedSubnetCidrs = new Set(
      lanMeshStore.subnets.filter((subnet) => !subnet.isLoopback).map((subnet) => subnet.cidr),
    );
  } catch {
    /* leave subnets empty; the LAN view surfaces the empty state */
  }

  // Seeds the initial state directly rather than waiting for the
  // background poller's first ~2s tick — otherwise the app could start
  // with Warcraft III already open and briefly show "detenido".
  try {
    useGameProcessStore().running = await isGameRunning();
  } catch {
    /* leave the default (false); the poller will correct it shortly */
  }

  try {
    const initialMuted = await getMutedPeers();
    useRoomsStore().updateMutedPeers(initialMuted);
  } catch {
    /* leave default empty set */
  }
}

window.addEventListener("beforeunload", () => {
  void stopWatchGames();
});
