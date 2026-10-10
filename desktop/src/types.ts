// Mirrors the Serialize-derived DTOs in src-tauri/src/{network,core::dto}.rs.
// Field names use camelCase to match #[serde(rename_all = "camelCase")].

export interface DetectedSubnet {
  interfaceName: string;
  ip: string;
  netmask: string;
  cidr: string;
  isLoopback: boolean;
}

export interface RelayInfo {
  addr: string; // "ip:port"
  serverName: string;
  protocolVersion: number;
  relayPort: number;
  knownPeers: number;
  uptimeSecs: number;
}

export interface GameInfo {
  sourceAddr: string;
  gameName: string | null;
  lastSeenSecsAgo: number;
  payloadLen: number;
}

export interface RelayStatus {
  running: boolean;
  peers: string[];
}

export interface LaunchResult {
  pid: number;
}

export function relayIp(relay: RelayInfo): string {
  const lastColon = relay.addr.lastIndexOf(":");
  return lastColon === -1 ? relay.addr : relay.addr.slice(0, lastColon);
}

// --- Hub mode (VPS matchmaking, multi-server) ---
// Mirrors src-tauri/src/{protocol/control_ws.rs, core/dto.rs}, which
// themselves must stay byte-for-byte identical to warlay-server's
// crates/protocol/src/control_ws.rs.

export type RoomStatus = "available" | "inProgress";

export interface RoomSummary {
  id: string;
  name: string;
  hostDisplayName: string;
  /** The room creator's persistent client id — compare against your own
   * (`getClientId()`) to tell whether this is a room you can delete. */
  hostClientId: string;
  gamePort: number;
  status: RoomStatus;
  playerCount: number;
  createdUnixSecs: number;
  hasPassword: boolean;
}

export interface RoomMember {
  clientId: string;
  displayName: string;
}

export type ServerMessage =
  | { type: "authenticated"; clientId: string; displayName: string }
  | { type: "roomList"; rooms: RoomSummary[] }
  | { type: "roomCreated"; room: RoomSummary; relayToken: string }
  | { type: "roomUpdated"; room: RoomSummary }
  | { type: "roomRemoved"; roomId: string }
  | { type: "joinedRoom"; room: RoomSummary; relayToken: string }
  | { type: "leftRoom" }
  | { type: "roomMembersChanged"; roomId: string; members: RoomMember[] }
  | { type: "serverStats"; connectedClients: number }
  /** Global per-server chat, not scoped to a room. `unixMillis` is a UTC
   * epoch timestamp — format it in the local machine's own timezone at
   * render time, never send/display it as UTC-for-everyone. */
  | { type: "chatMessage"; fromDisplayName: string; fromClientId: string; text: string; unixMillis: number }
  | {
      type: "privateMessage";
      fromDisplayName: string;
      fromClientId: string;
      toClientId: string;
      text: string;
      unixMillis: number;
    }
  | { type: "error"; message: string }
  | { type: "pong" };

export interface ChatMessageEntry {
  fromDisplayName: string;
  fromClientId: string;
  text: string;
  unixMillis: number;
}

export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

/** "" means launch with no `-graphicsapi` flag at all (the game's own
 * default) — see `utils/launch.ts`'s `buildLaunchArgs`. The other values are
 * WC3's own `-graphicsapi` argument spellings (capitalized exactly like
 * this), only supported from game patch 1.29 onward — older `gameVersion`s
 * ignore an unrecognized flag rather than failing to launch, so this isn't
 * gated on the configured version. */
export type GraphicsApi = "" | "Direct3D9" | "Direct3D11" | "OpenGL2";

export interface AppSettings {
  displayName: string;
  gameExecutablePath: string;
  gameArgs: string;
  gameVersion: number;
  slotsTotal: number;
  graphicsApi: GraphicsApi;
  /** Independent live-reloadable levels — see `log_control` on the backend
   * and `LogLevelSection.vue`. */
  consoleLogLevel: LogLevel;
  fileLogLevel: LogLevel;
  /** How many of the most recent chat messages each server's chat keeps in
   * memory client-side — chat is never persisted server-side or across a
   * restart (see `ChatSettingsSection.vue`), so this is the only retention
   * knob that exists. */
  chatMessageLimit: number;
  /** 0..1 multiplier applied on top of each sound's own base volume — see
   * `AudioSettingsSection.vue` and `utils/audio.ts`. */
  masterVolume: number;
  muted: boolean;
  /** "" plays through the OS default output — see `AudioSettingsSection.vue`. */
  audioOutputDeviceId: string;
  /** Log files older than this many days are deleted once at every launch,
   * and again immediately when this changes — see `LogLevelSection.vue`. */
  logRetentionDays: number;
  voiceEnabled: boolean;
  microphoneDeviceId: string;
  /** Toggles the microphone on/off on press — the only voice control key.
   * The mic always starts off; this turns it on, pressing it again mutes
   * back. See `AudioSettingsSection.vue`. */
  micToggleKey: string;
  voiceInputVolume: number;
  voiceOutputVolume: number;
}

/** Read-only hardware snapshot from `get_system_info` — informational only,
 * shown next to the "Motor gráfico" selector so the user has something to
 * go on. Windows-only for now; on other platforms every numeric field comes
 * back 0 and the GPU fields come back null. */
/** The daemon's (wnetd.exe) own persisted configuration — separate from
 * `AppSettings`, which is the desktop client's own settings. See
 * `daemon/src/storage.rs`. */
export interface AgentSettings {
  logLevel: LogLevel;
  lanDiscoveryEnabled: boolean;
}

/** One P2P mesh peer's direct-link negotiation state, for the "Agente"
 * diagnostic panel. */
export interface PeerLinkStatus {
  clientId: string;
  isHost: boolean;
  p2pAddr: string | null;
  state: "not_attempted" | "connecting" | "connected" | "failed";
}

/** Snapshot of what the daemon is doing right now — reply to
 * `getAgentStatus`, polled while the "Agente" panel is open. */
export interface AgentStatusSnapshot {
  uptimeSecs: number;
  hubConnections: number;
  tunnelActive: boolean;
  lanDiscoveryEnabled: boolean;
  meshPeers: PeerLinkStatus[];
}

export interface SystemInfo {
  cpuCores: number;
  totalRamMb: number;
  gpuName: string | null;
  gpuVramMb: number | null;
}

export interface ServerInfo {
  id: string;
  name: string;
  address: string;
  serverId: string | null;
  hasCredentials: boolean;
  lastConnectedAt: number | null;
}

export type ConnectStatus =
  | { status: "connected"; serverName: string }
  | { status: "passwordRequired"; serverName: string }
  | { status: "rejected"; message: string; retryAsNew: boolean };

// The app connects to every registered server at once (Discord-style), so
// every event a connection produces is tagged with which server it came
// from — mirrors src-tauri/src/core/dto.rs's `Server*Event` structs.
export interface ServerStatusEvent {
  serverId: string;
  connected: boolean;
}

export interface ServerPingEvent {
  serverId: string;
  pingMs: number;
}

export interface ServerMessageEvent {
  serverId: string;
  message: ServerMessage;
}

export type ServerConnectionStatus = "connecting" | "connected" | "passwordRequired" | "rejected" | "disconnected";

/** Matches warlay-server's `hub.tcp_tunnel_addr` default port. */
export const DEFAULT_HUB_TCP_TUNNEL_PORT = 7778;

/** Where a `wss://` hub exposes the same tunnel behind TLS (terminated by
 * the hub's reverse proxy in front of `hub.tcp_tunnel_addr`). */
export const DEFAULT_HUB_TLS_TUNNEL_PORT = 7443;

export interface ParsedServerAddress {
  host: string;
  secure: boolean;
}

/** Mirrors the daemon's `HubAddress`: a bare "host:port" (legacy, plain
 * ws) or a `ws://` / `wss://` URL. `null` when it's neither. */
export function parseServerAddress(address: string): ParsedServerAddress | null {
  const trimmed = address.trim();
  let url: URL;
  try {
    url = new URL(trimmed.includes("://") ? trimmed : `ws://${trimmed}`);
  } catch {
    return null;
  }
  if ((url.protocol !== "ws:" && url.protocol !== "wss:") || !url.hostname) return null;
  return { host: url.hostname.replace(/^\[|\]$/g, ""), secure: url.protocol === "wss:" };
}
