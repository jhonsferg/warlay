import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { rooms as api } from "../services/api";
import { ApiError } from "../services/http";
import { native, type PeerLinkStatus } from "../services/native";
import type { HelperInfo, RoomSession, RoomView } from "../services/types";
import { useSession } from "./session";
import { useSettings } from "./settings";

/** Lobby rooms, plus the room this player is in and its P2P session. */
export const useRooms = defineStore("rooms", () => {
  const list = ref<RoomView[]>([]);
  const session = ref<RoomSession | null>(null);
  const links = ref<PeerLinkStatus[]>([]);
  const tunnelActive = ref(false);
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  const current = computed(() => session.value?.room ?? null);
  const isHost = computed(() => session.value?.isHost ?? false);

  /** How each other member is reached: "direct", "relay" or still pending. */
  function linkOf(userId: string): PeerLinkStatus | undefined {
    return links.value.find((l) => l.clientId === userId);
  }

  async function loadList(query = "") {
    list.value = await api.list(query);
  }

  async function startNative(next: RoomSession) {
    session.value = next;
    try {
      await native.startRoomSession(next);
      syncOverlay(next.room);
      const settings = useSettings();
      if (!settings.loaded) await settings.load().catch(() => {});
      if (settings.voiceEnabled) await native.startVoiceChat(next.room.id).catch(() => {});
    } finally {
      startPolling();
    }
  }

  /** The in-game overlay shows the roster (and who's talking). */
  function syncOverlay(room: RoomView | null) {
    void native
      .setOverlayRoster(
        room?.id ?? null,
        (room?.members ?? []).map((m) => ({ clientId: m.id, displayName: m.displayName })),
      )
      .catch(() => {});
  }

  function startPolling() {
    if (pollTimer) return;
    const poll = async () => {
      try {
        const status = await native.agentStatus();
        links.value = status?.meshPeers ?? [];
        tunnelActive.value = status?.tunnelActive ?? false;
      } catch {
        /* daemon not reachable right now */
      }
    };
    void poll();
    pollTimer = setInterval(poll, 2000);
  }

  function stopPolling() {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = null;
    links.value = [];
    tunnelActive.value = false;
  }

  async function endLocal() {
    session.value = null;
    stopPolling();
    syncOverlay(null);
    await native.stopVoiceChat().catch(() => {});
    try {
      await native.stopRoomSession();
    } catch {
      /* nothing running */
    }
  }

  async function restoreCurrent() {
    const existing = await api.current();
    if (existing) await startNative(existing);
  }

  async function create(body: { name: string; communityId?: string | null; password?: string; maxPlayers?: number }) {
    const created = await api.create(body);
    upsert(created.room);
    await startNative(created);
    return created;
  }

  async function join(roomId: string, password?: string) {
    const joined = await api.join(roomId, password);
    upsert(joined.room);
    await startNative(joined);
    return joined;
  }

  /** Accepting a room invite already joined us; just start the session. */
  async function adopt(joined: RoomSession) {
    upsert(joined.room);
    await startNative(joined);
  }

  async function leave() {
    if (!session.value) return;
    const id = session.value.room.id;
    await endLocal();
    await api.leave(id);
  }

  async function close() {
    if (!session.value) return;
    const id = session.value.room.id;
    await endLocal();
    await api.close(id);
  }

  async function setStatus(status: "open" | "in_game") {
    if (!session.value?.isHost) return;
    const updated = await api.setStatus(session.value.room.id, status);
    upsert(updated);
  }

  const invite = (userId: string) => (session.value ? api.invite(session.value.room.id, userId) : Promise.resolve());
  const kick = (userId: string) => (session.value ? api.kick(session.value.room.id, userId) : Promise.resolve());

  /** Opens the game tunnel over the mesh, then starts Warcraft III. */
  async function play(executablePath: string, args: string) {
    if (!session.value) return;
    await native.startGameTunnel(session.value.room.gamePort);
    tunnelActive.value = true;
    if (executablePath) {
      await native.launchGame(
        executablePath,
        args.split(" ").filter((a) => a.trim()),
      );
    }
  }

  /** Room updates are coalesced on the server and can arrive after a newer
   * state we already hold (e.g. the reply to our own join): an older
   * snapshot must not overwrite it -- nor make us think we were kicked. */
  function isStale(room: RoomView, known: RoomView | undefined) {
    return room.snapshotAt !== undefined && known?.snapshotAt !== undefined && room.snapshotAt < known.snapshotAt;
  }

  function upsert(room: RoomView) {
    if (
      isStale(
        room,
        list.value.find((r) => r.id === room.id),
      )
    )
      return;
    if (room.status === "closed") {
      list.value = list.value.filter((r) => r.id !== room.id);
      return;
    }
    const index = list.value.findIndex((r) => r.id === room.id);
    if (index >= 0) list.value[index] = room;
    else list.value.unshift(room);
  }

  // Realtime handlers.
  function applyUpdated(room: RoomView) {
    upsert(room);
    if (session.value?.room.id === room.id) {
      if (isStale(room, session.value.room)) return null;
      const me = useSession().me;
      if (me && !room.members.some((m) => m.id === me.id)) {
        void endLocal();
        return "kicked" as const;
      }
      session.value = { ...session.value, room };
      syncOverlay(room);
    }
    return null;
  }

  /** The room's helper died and another took over: move the mesh there
   * (links already up keep running, so a match in progress isn't cut). */
  function applyHelperChanged(roomId: string, helper: HelperInfo) {
    if (session.value?.room.id !== roomId) return;
    session.value = { ...session.value, helper };
    void native.updateRoomHelper(helper).catch(() => {});
  }

  /** After the realtime socket comes back: a helper change pushed while it
   * was down is lost -- and it's often down precisely because its edge was
   * that same dead helper -- so ask the main server where the room is now. */
  async function resyncSession() {
    const current = session.value;
    if (!current) return;
    let fresh: RoomSession;
    try {
      fresh = await api.session(current.room.id);
    } catch (e) {
      // Closed, or we were removed, while we weren't listening.
      if (
        e instanceof ApiError &&
        (e.status === 404 || e.status === 403) &&
        session.value?.room.id === current.room.id
      ) {
        await endLocal();
      }
      return;
    }
    if (session.value?.room.id !== fresh.room.id) return;
    upsert(fresh.room);
    if (fresh.helper.id !== session.value.helper.id) applyHelperChanged(fresh.room.id, fresh.helper);
  }

  function applyClosed(roomId: string) {
    list.value = list.value.filter((r) => r.id !== roomId);
    if (session.value?.room.id === roomId) {
      void endLocal();
      return true;
    }
    return false;
  }

  function reset() {
    list.value = [];
    void endLocal();
  }

  return {
    list,
    session,
    current,
    isHost,
    links,
    tunnelActive,
    linkOf,
    loadList,
    restoreCurrent,
    resyncSession,
    create,
    join,
    adopt,
    leave,
    close,
    setStatus,
    invite,
    kick,
    play,
    applyUpdated,
    applyHelperChanged,
    applyClosed,
    reset,
  };
});
