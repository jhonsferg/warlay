import { currentEdge, edgeBase, nextEdge, refreshEdges } from "./edges";
import { accessToken, mainServerUrl, refreshTokens } from "./http";
import type { ChatMessage, RealtimeEvent } from "./types";

type Handler = (event: RealtimeEvent) => void;

const PING_EVERY_MS = 10_000;
/** Silence (not even a pong) for this long means the edge is gone without
 * closing the socket (frozen node, network partition): drop it and move on
 * instead of waiting for TCP to give up, which can take many minutes. */
const DEAD_AFTER_MS = 25_000;
export type ConnectionState = "idle" | "connecting" | "open" | "reconnecting";

interface Pending {
  resolve: (message: ChatMessage) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

/** The realtime socket. It lives on a helper edge (falling back to the main
 * server), moves to another edge when its own disappears, keeps itself
 * alive with pings, fans events out to subscribers, and carries chat sends
 * so chat keeps working while the main server is down. */
class Realtime {
  private socket: WebSocket | null = null;
  private handlers = new Set<Handler>();
  private stateHandlers = new Set<(state: ConnectionState) => void>();
  private pending = new Map<string, Pending>();
  private retry = 0;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private pingTimer: ReturnType<typeof setInterval> | null = null;
  private wanted = false;
  /** Every edge failed: use the main server until the next refresh. */
  private onMain = false;
  state: ConnectionState = "idle";
  /** Where the socket is connected (edge name or "principal"). */
  via = "";

  on(handler: Handler): () => void {
    this.handlers.add(handler);
    return () => this.handlers.delete(handler);
  }

  onState(handler: (state: ConnectionState) => void): () => void {
    this.stateHandlers.add(handler);
    return () => this.stateHandlers.delete(handler);
  }

  private setState(state: ConnectionState) {
    this.state = state;
    this.stateHandlers.forEach((h) => h(state));
  }

  async connect() {
    this.wanted = true;
    if (this.socket) return;
    if (this.retry === 0 && !this.onMain) await refreshEdges();
    const token = accessToken();
    if (!token || !this.wanted || this.socket) return;
    this.setState(this.retry === 0 ? "connecting" : "reconnecting");
    const base = this.onMain ? mainServerUrl() : edgeBase();
    this.via = this.onMain ? "principal" : (currentEdge()?.name ?? "principal");
    const socket = new WebSocket(`${base.replace(/^http/, "ws")}/api/v1/ws?token=${encodeURIComponent(token)}`);
    this.socket = socket;
    let opened = false;
    let lastHeard = Date.now();

    socket.onopen = () => {
      opened = true;
      lastHeard = Date.now();
      this.retry = 0;
      this.setState("open");
      this.pingTimer = setInterval(() => {
        if (Date.now() - lastHeard > DEAD_AFTER_MS) {
          // close() on a dead link only fires onclose after the closing
          // handshake times out: detach and reconnect right away.
          socket.onclose = null;
          socket.onmessage = null;
          socket.close();
          onClosed();
          return;
        }
        if (socket.readyState === WebSocket.OPEN) socket.send("ping");
      }, PING_EVERY_MS);
    };
    socket.onmessage = (msg) => {
      lastHeard = Date.now();
      if (msg.data === "pong") return;
      let event: { type: string; requestId?: string; message?: unknown };
      try {
        event = JSON.parse(msg.data as string);
      } catch {
        return;
      }
      if (event.type === "chat.ack" || event.type === "chat.error") {
        const waiter = event.requestId ? this.pending.get(event.requestId) : undefined;
        if (!waiter) return;
        this.pending.delete(event.requestId!);
        clearTimeout(waiter.timer);
        if (event.type === "chat.ack") waiter.resolve(event.message as ChatMessage);
        else waiter.reject(new Error(String(event.message ?? "No se pudo enviar")));
        return;
      }
      this.handlers.forEach((h) => h(event as RealtimeEvent));
    };
    const onClosed = () => {
      if (this.socket !== socket) return;
      this.cleanup();
      if (!this.wanted) return;
      this.setState("reconnecting");
      // The edge went away (or never answered): try the next one, then the
      // main server, then start over with a fresh list.
      if (!this.onMain && !nextEdge()) this.onMain = true;
      else if (this.onMain && !opened) this.onMain = false;
      const delay = opened ? 250 : Math.min(15_000, 500 * 2 ** this.retry++);
      this.retryTimer = setTimeout(async () => {
        // The socket may also have died because the access token expired.
        if (this.retry > 2) await refreshTokens();
        void this.connect();
      }, delay);
    };
    socket.onclose = onClosed;
  }

  /** Sends a chat message over the socket; resolves with the stored message.
   * `requestId` identifies the send: retried with the same id (e.g. over
   * HTTP) the server never posts it twice. */
  sendChat(
    kind: string,
    targetId: string,
    body: string,
    requestId: string = crypto.randomUUID(),
  ): Promise<ChatMessage> {
    const socket = this.socket;
    if (!socket || socket.readyState !== WebSocket.OPEN) return Promise.reject(new Error("socket closed"));
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(requestId);
        reject(new Error("socket timeout"));
      }, 8000);
      this.pending.set(requestId, { resolve, reject, timer });
      socket.send(JSON.stringify({ type: "chat.send", requestId, kind, targetId, body }));
    });
  }

  private cleanup() {
    if (this.pingTimer) clearInterval(this.pingTimer);
    this.pingTimer = null;
    this.socket = null;
    for (const [id, waiter] of this.pending) {
      clearTimeout(waiter.timer);
      waiter.reject(new Error("socket closed"));
      this.pending.delete(id);
    }
  }

  disconnect() {
    this.wanted = false;
    if (this.retryTimer) clearTimeout(this.retryTimer);
    this.retry = 0;
    this.onMain = false;
    this.socket?.close();
    this.cleanup();
    this.setState("idle");
  }
}

export const realtime = new Realtime();
