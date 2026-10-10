import { defineStore } from "pinia";
import { ref } from "vue";
import {
  addServer as apiAddServer,
  connectToServer,
  removeServer as apiRemoveServer,
  sendChatMessage as apiSendChatMessage,
  sendPrivateMessage as apiSendPrivateMessage,
} from "../api";
import type { ChatMessageEntry, RoomSummary, ServerConnectionStatus, ServerInfo } from "../types";
import { useRoomsStore } from "./rooms";
import { useSettingsStore } from "./settings";
import { useUiStore } from "./ui";
import { playConnected, playAlert } from "../utils/audio";

/** Everything about one registered server's live connection — the app
 * connects to every registered server at once (Discord-style), so this is
 * one entry per server rather than a single global set of fields. */
export interface ServerConnectionState {
  status: ServerConnectionStatus;
  authenticated: boolean;
  pingMs: number | null;
  connectedClients: number | null;
  rooms: Map<string, RoomSummary>;
  errorMessage: string | null;
  /** Global chat for this server (not per-room) — capped at
   * `settingsStore.settings.chatMessageLimit` (configurable, see
   * `ChatSettingsSection.vue`). No history: only messages sent while this
   * client was connected, never persisted server-side or across a restart —
   * same tradeoff Steam/SC2 make for ephemeral lobby/channel chat. */
  chatMessages: ChatMessageEntry[];
}

export function freshConnectionState(): ServerConnectionState {
  return {
    status: "connecting",
    authenticated: false,
    pingMs: null,
    connectedClients: null,
    rooms: new Map(),
    errorMessage: null,
    chatMessages: [],
  };
}

export const useServersStore = defineStore("servers", () => {
  const settingsStore = useSettingsStore();
  const uiStore = useUiStore();

  const servers = ref(new Map<string, ServerInfo>());
  const serverConnections = ref(new Map<string, ServerConnectionState>());

  async function addServer(address: string): Promise<ServerInfo> {
    const server = await apiAddServer(address);
    servers.value.set(server.id, server);
    serverConnections.value.set(server.id, freshConnectionState());
    void connectServer(server.id);
    return server;
  }

  /** Connects one server's slot — used both for the auto-connect sweep at
   * launch and for a manual retry once the user has entered a password. */
  async function connectServer(serverId: string, password?: string): Promise<void> {
    const connection = serverConnections.value.get(serverId);
    if (!connection) return;
    if (!settingsStore.settings.displayName.trim()) {
      settingsStore.pendingGuidance = "Configura tu nombre de guerra antes de conectar a un servidor.";
      uiStore.railSelection = { kind: "settings" };
      return;
    }
    connection.status = "connecting";
    try {
      const result = await connectToServer(serverId, password);
      if (result.status === "connected") {
        connection.status = "connected";
        connection.errorMessage = null;
        // A server that hasn't configured its own name reports "" here --
        // keep whatever we already had (the address-based default from
        // add_server, e.g. "Server (host:port)") rather than blanking it
        // out to an empty string that then renders as nothing/"undefined".
        const server = servers.value.get(serverId);
        if (server && result.serverName?.trim()) server.name = result.serverName;
        playConnected();
      } else if (result.status === "passwordRequired") {
        connection.status = "passwordRequired";
      } else {
        connection.status = result.retryAsNew ? "disconnected" : "rejected";
        connection.errorMessage = result.message;
        playAlert();
        if (result.retryAsNew) {
          await connectServer(serverId);
        }
      }
    } catch (error) {
      connection.status = "disconnected";
      connection.errorMessage = `Error al conectar: ${String(error)}`;
      playAlert();
    }
  }

  /** The only way a server's connection ever goes away deliberately — no
   * manual "Desconectar" exists anymore, connections are auto-managed.
   * Blocks (client-side, mirroring the backend's authoritative check)
   * removing the server that currently owns the one active game session.
   * Calls `useRoomsStore()` lazily inside the function body (not at this
   * store's setup top level) — `rooms.ts` composes `useServersStore()` at
   * *its* setup's top level, so this side of the cycle has to be the one
   * that defers, or whichever store initializes second would see the
   * other's setup only partially complete. */
  async function removeServerFlow(serverId: string): Promise<void> {
    const roomsStore = useRoomsStore();
    if (roomsStore.activeRoom?.serverId === serverId) {
      throw new Error("Termina o abandona la partida activa antes de eliminar este servidor.");
    }
    await apiRemoveServer(serverId);
    servers.value.delete(serverId);
    serverConnections.value.delete(serverId);
    if (uiStore.railSelection?.kind === "server" && uiStore.railSelection.serverId === serverId) {
      const next = [...servers.value.keys()][0];
      uiStore.railSelection = next ? { kind: "server", serverId: next } : null;
    }
  }

  /** The hub broadcasts the resulting `chatMessage` back to every connected
   * client including the sender, so nothing needs to be appended locally
   * here — `bootstrap.ts`'s `handleServerMessage` does that uniformly. */
  async function sendChatMessage(serverId: string, text: string): Promise<void> {
    const trimmed = text.trim();
    if (!trimmed) return;
    await apiSendChatMessage(serverId, trimmed);
  }

  async function sendPrivateMessage(serverId: string, toClientId: string, text: string): Promise<void> {
    const trimmed = text.trim();
    if (!trimmed) return;
    await apiSendPrivateMessage(serverId, toClientId, trimmed);
  }

  return {
    servers,
    serverConnections,
    addServer,
    connectServer,
    removeServerFlow,
    sendChatMessage,
    sendPrivateMessage,
  };
});
