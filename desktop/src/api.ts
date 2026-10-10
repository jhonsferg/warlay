import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AgentSettings,
  AgentStatusSnapshot,
  AppSettings,
  ConnectStatus,
  DetectedSubnet,
  GameInfo,
  LaunchResult,
  LogLevel,
  RelayInfo,
  RelayStatus,
  RoomMember,
  ServerInfo,
  ServerMessageEvent,
  ServerPingEvent,
  ServerStatusEvent,
  SystemInfo,
} from "./types";

// Typed wrappers around every Tauri command exposed by src-tauri/src/commands.
// Keeping the invoke("name", args) calls in one file means a renamed or
// re-signatured Rust command only has to be fixed in a single place here.

export function listLocalSubnets(): Promise<DetectedSubnet[]> {
  return invoke("list_local_subnets");
}

export function discoverRelays(cidrRanges: string[], controlPort: number, timeoutMs: number): Promise<RelayInfo[]> {
  return invoke("discover_relays", { cidrRanges, controlPort, timeoutMs });
}

export function listGames(relayIp: string, controlPort: number): Promise<GameInfo[]> {
  return invoke("list_games", { relayIp, controlPort });
}

export function watchGames(relayIp: string, controlPort: number, intervalMs: number): Promise<void> {
  return invoke("watch_games", { relayIp, controlPort, intervalMs });
}

export function stopWatchGames(): Promise<void> {
  return invoke("stop_watch_games");
}

export function startLocalRelay(bindAddr: string, peers: string[]): Promise<void> {
  return invoke("start_local_relay", { bindAddr, peers });
}

export function stopLocalRelay(): Promise<void> {
  return invoke("stop_local_relay");
}

export function launchGame(executablePath: string, args: string[], workingDir: string | null): Promise<LaunchResult> {
  return invoke("launch_game", { executablePath, args, workingDir });
}

export function pathExists(path: string): Promise<boolean> {
  return invoke("path_exists", { path });
}

export function isGameRunning(): Promise<boolean> {
  return invoke("is_game_running");
}

// Real-time event subscriptions emitted from the Rust backend.

export function onRelayFound(handler: (relay: RelayInfo) => void): Promise<UnlistenFn> {
  return listen<RelayInfo>("discovery://relay-found", (event) => handler(event.payload));
}

export function onDiscoveryCompleted(handler: (total: number) => void): Promise<UnlistenFn> {
  return listen<number>("discovery://completed", (event) => handler(event.payload));
}

export function onGamesUpdated(handler: (games: GameInfo[]) => void): Promise<UnlistenFn> {
  return listen<GameInfo[]>("games://updated", (event) => handler(event.payload));
}

export function onRelayStatusChanged(handler: (status: RelayStatus) => void): Promise<UnlistenFn> {
  return listen<RelayStatus>("relay://status-changed", (event) => handler(event.payload));
}

// --- Daemon lifecycle (the startup loading screen calls these before
// showing the rest of the UI) ---

export function checkDaemonReady(): Promise<boolean> {
  return invoke("check_daemon_ready");
}

export function ensureDaemonRunning(): Promise<void> {
  return invoke("ensure_daemon_running");
}

// --- Hub mode (VPS matchmaking, multi-server) commands ---

export function listServers(): Promise<ServerInfo[]> {
  return invoke("list_servers");
}

export function addServer(address: string): Promise<ServerInfo> {
  return invoke("add_server", { address });
}

export function removeServer(serverId: string): Promise<void> {
  return invoke("remove_server", { serverId });
}

export function getClientId(): Promise<string> {
  return invoke("get_client_id");
}

export function connectToServer(serverId: string, password?: string): Promise<ConnectStatus> {
  return invoke("connect_to_server", { serverId, password: password ?? null });
}

export function disconnectFromServer(serverId: string): Promise<void> {
  return invoke("disconnect_from_server", { serverId });
}

export function createRoom(serverId: string, name: string, gamePort: number, password?: string): Promise<void> {
  return invoke("create_room", { serverId, name, gamePort, password: password ?? null });
}

export function joinRoom(serverId: string, roomId: string, password?: string): Promise<void> {
  return invoke("join_room", { serverId, roomId, password: password ?? null });
}

export function leaveRoom(serverId: string): Promise<void> {
  return invoke("leave_room", { serverId });
}

export function deleteRoom(serverId: string, roomId: string): Promise<void> {
  return invoke("delete_room", { serverId, roomId });
}

export function setRoomInProgress(serverId: string): Promise<void> {
  return invoke("set_room_in_progress", { serverId });
}

export function setRoomAvailable(serverId: string): Promise<void> {
  return invoke("set_room_available", { serverId });
}

export function sendChatMessage(serverId: string, text: string): Promise<void> {
  return invoke("send_chat_message", { serverId, text });
}

export function sendPrivateMessage(serverId: string, toClientId: string, text: string): Promise<void> {
  return invoke("send_private_message", { serverId, toClientId, text });
}

export function startHostingTunnel(
  hubHost: string,
  hubTcpPort: number,
  roomToken: string,
  gamePort: number,
  secure: boolean,
): Promise<void> {
  return invoke("start_hosting_tunnel", { hubHost, hubTcpPort, roomToken, gamePort, secure });
}

export function startJoiningTunnel(
  hubHost: string,
  hubTcpPort: number,
  roomToken: string,
  gamePort: number,
  secure: boolean,
): Promise<void> {
  return invoke("start_joining_tunnel", { hubHost, hubTcpPort, roomToken, gamePort, secure });
}

export function stopHubTunnel(): Promise<void> {
  return invoke("stop_hub_tunnel");
}

/** Current truth for the active room's roster — see `OverlayApp.vue`'s use
 * of this on mount, and `commands::matchmaking::get_room_members`'s doc
 * comment for why it needs to exist at all. */
export function getRoomMembers(): Promise<RoomMember[]> {
  return invoke("get_room_members");
}

export function loadSettings(): Promise<AppSettings> {
  return invoke("load_settings");
}

export function saveSettings(settings: AppSettings): Promise<void> {
  return invoke("save_settings", { settings });
}

/** Applies immediately on the backend (see `log_control`) and persists —
 * no restart needed, no separate save step. */
export function setLogLevel(target: "console" | "file", level: LogLevel): Promise<void> {
  return invoke("set_log_level", { target, level });
}

/** Read-only, queried once when the "Sonido"/"Motor gráfico" section of
 * Ajustes mounts — see `SystemInfoHint.vue`. */
export function getSystemInfo(): Promise<SystemInfo> {
  return invoke("get_system_info");
}

export function getAgentSettings(): Promise<AgentSettings> {
  return invoke("get_agent_settings");
}

/** Applies immediately on the daemon (log level reload, LAN discovery
 * toggle) and persists — see `daemon/src/log_control.rs` and
 * `daemon/src/storage.rs`. */
export function saveAgentSettings(settings: AgentSettings): Promise<void> {
  return invoke("save_agent_settings", { settings });
}

export function getAgentStatus(): Promise<AgentStatusSnapshot> {
  return invoke("get_agent_status");
}

/** Resets any P2P mesh peer stuck in "failed" back to "not_attempted" so
 * the daemon's own retry loop picks it up again, without leaving/rejoining
 * the room. */
export function retryPeerMesh(): Promise<void> {
  return invoke("retry_peer_mesh");
}

/** Persists the retention window and re-runs the log cleanup sweep with it
 * immediately (see `log_cleanup` on the backend) — no restart needed. */
export function setLogRetentionDays(days: number): Promise<void> {
  return invoke("set_log_retention_days", { days });
}

// --- Hub mode events ---
// Tagged with `serverId` since the app can be connected to several servers
// at once — see `ServerStatusEvent`/`ServerPingEvent`/`ServerMessageEvent`.

export function onMatchmakingStatus(handler: (event: ServerStatusEvent) => void): Promise<UnlistenFn> {
  return listen<ServerStatusEvent>("matchmaking://status", (event) => handler(event.payload));
}

export function onMatchmakingPing(handler: (event: ServerPingEvent) => void): Promise<UnlistenFn> {
  return listen<ServerPingEvent>("matchmaking://ping", (event) => handler(event.payload));
}

export function onMatchmakingMessage(handler: (event: ServerMessageEvent) => void): Promise<UnlistenFn> {
  return listen<ServerMessageEvent>("matchmaking://message", (event) => handler(event.payload));
}

export function onHubTunnelStatusChanged(handler: (running: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("hubTunnel://status-changed", (event) => handler(event.payload));
}

export function onGameExited(handler: (pid: number) => void): Promise<UnlistenFn> {
  return listen<number>("game://exited", (event) => handler(event.payload));
}

export function onGameRunningStateChanged(handler: (running: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("game://running-state-changed", (event) => handler(event.payload));
}

/** Every log line the Rust backend produces, forwarded live — see
 * src-tauri/src/log_bridge.rs. Powers the "Consola" tab. */
export function onLogLine(handler: (line: string) => void): Promise<UnlistenFn> {
  return listen<string>("log://line", (event) => handler(event.payload));
}

// --- Voice chat commands & events ---

export function onVoiceFrameIn(
  handler: (event: { fromClientId: string; opusBytes: number[] }) => void,
): Promise<UnlistenFn> {
  return listen<{ fromClientId: string; opusBytes: number[] }>("voice://frame-in", (event) => handler(event.payload));
}

export function pushVoiceFrame(fromClientId: string, opusBytes: number[]): Promise<void> {
  return invoke("push_voice_frame", { fromClientId, opusBytes });
}

export function startVoiceChat(
  roomToken: string,
  inputDeviceId: string | null,
  outputDeviceId: string | null,
): Promise<void> {
  return invoke("start_voice_chat", { roomToken, inputDeviceId, outputDeviceId });
}

export function stopVoiceChat(): Promise<void> {
  return invoke("stop_voice_chat");
}

export function setOverlayInteractive(interactive: boolean): Promise<void> {
  return invoke("set_overlay_interactive", { interactive });
}

export function toggleMutePeer(clientId: string): Promise<void> {
  return invoke("toggle_mute_peer", { clientId });
}

export function getMutedPeers(): Promise<string[]> {
  return invoke("get_muted_peers");
}

export function onMuteChanged(handler: (mutedClientIds: string[]) => void): Promise<UnlistenFn> {
  return listen<string[]>("voice://mute-changed", (event) => handler(event.payload));
}

export function toggleSelfMicMute(): Promise<boolean> {
  return invoke("toggle_self_mic_mute");
}

export function isSelfMicMuted(): Promise<boolean> {
  return invoke("is_self_mic_muted");
}

export function onSelfMuteChanged(handler: (muted: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("voice://self-mute-changed", (event) => handler(event.payload));
}
