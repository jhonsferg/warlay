// Bridge to the desktop shell (Tauri) and, through it, the daemon that does
// the actual networking. In a plain browser (vite dev, Playwright) there is
// no shell: every call is recorded and answered by a harmless stand-in, so
// the whole UI can be exercised and screenshotted without Windows.
import type { HelperInfo, RoomSession } from "./types";

export interface PeerLinkStatus {
  clientId: string;
  isHost: boolean;
  p2pAddr: string | null;
  state: "not_attempted" | "connecting" | "connected" | "relayed" | "failed";
}

export interface AgentStatus {
  uptimeSecs: number;
  hubConnections: number;
  tunnelActive: boolean;
  lanDiscoveryEnabled: boolean;
  meshPeers: PeerLinkStatus[];
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
    __warlayNative?: { calls: NativeCall[]; status: AgentStatus };
  }
}

function browserStub(): { calls: NativeCall[]; status: AgentStatus } {
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
  if (command === "get_agent_status") return stub.status as T;
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
  /** End-to-end encryption: keys stay in the shell; only public keys and text cross. */
  e2eIdentity: (userId: string) => invoke<string | undefined>("e2e_identity", { userIdText: userId }),
  e2eEncrypt: (me: string, peerId: string, peerPublicKey: string, requestId: string, plaintext: string) =>
    invoke<string>("e2e_encrypt", { me, peerId, peerPublicKey, requestId, plaintext }),
  e2eDecrypt: (me: string, peerId: string, peerPublicKey: string, senderId: string, envelope: string) =>
    invoke<{ plaintext: string; requestId: string }>("e2e_decrypt", { me, peerId, peerPublicKey, senderId, envelope }),
  e2eSafetyNumber: (me: string, peerPublicKey: string) =>
    invoke<string>("e2e_safety_number", { me, peerPublicKey }),

  ensureDaemonRunning: () => invoke<void>("ensure_daemon_running"),

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
  setOverlayRoster: (roomId: string | null, members: { clientId: string; displayName: string }[]) =>
    invoke<void>("set_overlay_roster", { roomId, members }),
  startVoiceChat: (roomToken: string) =>
    invoke<void>("start_voice_chat", { roomToken, inputDeviceId: null, outputDeviceId: null }),
  stopVoiceChat: () => invoke<void>("stop_voice_chat"),
  startGameTunnel: (gamePort: number) => invoke<void>("start_game_tunnel", { gamePort }),

  launchGame: (path: string, args: string[]) =>
    invoke<unknown>("launch_game", { executablePath: path, args, workingDir: null }),
  agentStatus: () => invoke<AgentStatus>("get_agent_status"),
  retryPeerMesh: () => invoke<void>("retry_peer_mesh"),

  loadSettings: () => invoke<GameSettings & Record<string, unknown>>("load_settings"),
  saveSettings: (settings: Record<string, unknown>) => invoke<void>("save_settings", { settings }),
  pickExecutable: async (): Promise<string | null> => {
    if (!isDesktop) return null;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ multiple: false, filters: [{ name: "Warcraft III", extensions: ["exe"] }] });
    return typeof picked === "string" ? picked : null;
  },

  onGameStarted: (handler: () => void) => listen<void>("session://game-started", handler),
  onGameEnded: (handler: () => void) => listen<void>("session://game-ended", handler),
  onTunnelStatus: (handler: (running: boolean) => void) => listen<boolean>("hubTunnel://status-changed", handler),
};
