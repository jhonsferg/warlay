// Bridge to the desktop shell (Tauri) and, through it, the agent that does
// the actual networking. In a plain browser (vite dev, Playwright) there is
// no shell: every call is recorded and answered by a harmless stand-in, so
// the whole UI can be exercised and screenshotted without Windows.
import type { NetworkStatus } from "./networkStatus";
import type { HelperInfo, RoomSession } from "./types";

export type { NetworkStatus, PeerLinkStatus } from "./networkStatus";

/** The network agent's own configuration (it applies a new log level at once). */
export interface AgentSettings {
  logLevel: string;
  lanDiscoveryEnabled: boolean;
}

/** What the log folder holds (the app's and the agent's logs). */
export interface LogUsage {
  files: number;
  bytes: number;
  /** Files the sweep that produced this reading removed. */
  removed: number;
}

export interface GameSettings {
  gameExecutablePath: string;
  gameArgs: string;
  voiceEnabled?: boolean;
}

type Unlisten = () => void;

export const isDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

interface NativeCall {
  command: string;
  args: Record<string, unknown>;
}

declare global {
  interface Window {
    __warlayNative?: { calls: NativeCall[]; status: NetworkStatus };
  }
}

function browserStub(): { calls: NativeCall[]; status: NetworkStatus } {
  window.__warlayNative ??= {
    calls: [],
    status: { uptimeSecs: 0, hubConnections: 0, tunnelActive: false, lanDiscoveryEnabled: true, meshPeers: [] },
  };
  return window.__warlayNative;
}

async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (isDesktop) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(command, args);
  }
  const stub = browserStub();
  stub.calls.push({ command, args });
  if (command === "get_network_status") return stub.status as T;
  if (command === "load_settings") {
    const raw = localStorage.getItem("warlay.browserSettings");
    return (raw ? JSON.parse(raw) : { gameExecutablePath: "", gameArgs: "" }) as T;
  }
  if (command === "save_settings") {
    localStorage.setItem("warlay.browserSettings", JSON.stringify(args.settings));
  }
  return undefined as T;
}

async function listen<T>(event: string, handler: (payload: T) => void): Promise<Unlisten> {
  if (!isDesktop) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, (e) => handler(e.payload));
}

export const native = {
  /** Developer tools: F12 / Ctrl+Shift+I, enabled in release builds too. */
  openDevtools: () => invoke<void>("open_devtools"),

  /** Logs: levels change while running; old files are swept by age. */
  agentSettings: () => invoke<AgentSettings | undefined>("get_agent_settings"),
  saveAgentSettings: (settings: AgentSettings) => invoke<void>("save_agent_settings", { settings }),
  setLogLevel: (target: "console" | "file", level: string) => invoke<void>("set_log_level", { target, level }),
  setLogRetentionDays: (days: number) => invoke<void>("set_log_retention_days", { days }),
  logUsage: () => invoke<LogUsage | undefined>("get_log_usage"),
  cleanLogsNow: (allInactive: boolean) => invoke<LogUsage | undefined>("clean_logs_now", { allInactive }),

  /** End-to-end encryption: keys stay in the shell; only public keys and text cross. */
  e2eIdentity: (userId: string) => invoke<string | undefined>("e2e_identity", { userIdText: userId }),
  e2eEncrypt: (me: string, peerId: string, peerPublicKey: string, requestId: string, plaintext: string) =>
    invoke<string>("e2e_encrypt", { me, peerId, peerPublicKey, requestId, plaintext }),
  e2eDecrypt: (me: string, peerId: string, peerPublicKey: string, senderId: string, envelope: string) =>
    invoke<{ plaintext: string; requestId: string }>("e2e_decrypt", { me, peerId, peerPublicKey, senderId, envelope }),
  e2eSafetyNumber: (me: string, peerPublicKey: string) => invoke<string>("e2e_safety_number", { me, peerPublicKey }),

  startRoomSession: (session: RoomSession) =>
    invoke<void>("start_room_session", {
      roomId: session.room.id,
      userId: session.userId,
      isHost: session.isHost,
      rendezvousAddr: session.helper.rendezvousAddr,
      relayAddr: session.helper.relayAddr,
      ticket: session.ticket,
    }),
  stopRoomSession: () => invoke<void>("stop_room_session"),
  updateRoomHelper: (helper: HelperInfo) =>
    invoke<void>("update_room_helper", { rendezvousAddr: helper.rendezvousAddr, relayAddr: helper.relayAddr }),
  startGameTunnel: (gamePort: number) => invoke<void>("start_game_tunnel", { gamePort }),

  launchGame: (path: string, args: string[]) =>
    invoke<unknown>("launch_game", { executablePath: path, args, workingDir: null }),
  /** What the agent is doing right now (P2P links, tunnel, hubs): read once... */
  networkStatus: () => invoke<NetworkStatus>("get_network_status"),
  /** ...then pushed by the backend whenever it changes (no polling). */
  onNetworkStatus: (handler: (status: NetworkStatus) => void) => listen<NetworkStatus>("network://status", handler),
  retryPeerMesh: () => invoke<void>("retry_peer_mesh"),

  loadSettings: () => invoke<GameSettings & Record<string, unknown>>("load_settings"),
  saveSettings: (settings: Record<string, unknown>) => invoke<void>("save_settings", { settings }),
  pickExecutable: async (): Promise<string | null> => {
    if (!isDesktop) return null;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ multiple: false, filters: [{ name: "Warcraft III", extensions: ["exe"] }] });
    return typeof picked === "string" ? picked : null;
  },

  /** One encoded Opus frame of our microphone, sent to the room over the mesh. */
  sendVoiceFrame: (frame: Uint8Array) =>
    invoke<number>("send_voice_frame", frame as unknown as Record<string, unknown>),
  onVoiceFrames: (handler: (frames: { from: string; data: string }[]) => void) =>
    listen<{ from: string; data: string }[]>("voice://frames", handler),
  setMicConsent: (granted: boolean) => invoke<void>("set_mic_consent", { granted }),
  openMicPrivacySettings: () => invoke<void>("open_mic_privacy_settings"),
  /** Starts sharing this PC's sound with the room (Warlay's own audio excluded). */
  startDesktopAudio: () => invoke<void>("start_desktop_audio"),
  stopDesktopAudio: () => invoke<void>("stop_desktop_audio"),
  /** Other members' shared desktop audio (stereo), separate from their voice. */
  onDesktopFrames: (handler: (frames: { from: string; data: string }[]) => void) =>
    listen<{ from: string; data: string }[]>("voice://desktop-frames", handler),
  registerVoiceShortcut: (accelerator?: string) => invoke<string>("register_voice_shortcut", { accelerator }),
  unregisterVoiceShortcut: () => invoke<void>("unregister_voice_shortcut"),
  /** The global mute shortcut was pressed (works while the game has focus). */
  onVoiceToggle: (handler: () => void) => listen<void>("voice://toggle-mute", handler),

  onGameStarted: (handler: () => void) => listen<void>("session://game-started", handler),
  onGameEnded: (handler: () => void) => listen<void>("session://game-ended", handler),
  onTunnelStatus: (handler: (running: boolean) => void) => listen<boolean>("hubTunnel://status-changed", handler),
};
