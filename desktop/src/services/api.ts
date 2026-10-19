// Typed endpoints of the main server.
import { edgeBase } from "./edges";
import { api, ApiError } from "./http";
import { realtime } from "./realtime";
import type {
  AppNotification,
  AuthResponse,
  ChannelKind,
  ChatMessage,
  CommunityDetail,
  CommunitySummary,
  FriendRequests,
  InviteView,
  Me,
  Profile,
  RoomSession,
  RoomView,
  Tokens,
  UserWithPresence,
} from "./types";

export const auth = {
  register: (body: { username: string; email: string; password: string; displayName?: string }) =>
    api<AuthResponse>("POST", "/auth/register", body),
  login: (login: string, password: string) => api<AuthResponse>("POST", "/auth/login", { login, password }),
  logout: () => api<void>("POST", "/auth/logout"),
  me: () => api<Me>("GET", "/me"),
  updateMe: (body: { displayName?: string; bio?: string; avatarColor?: string }) => api<Me>("PATCH", "/me", body),
  changePassword: (currentPassword: string, newPassword: string) =>
    api<void>("POST", "/me/password", { currentPassword, newPassword }),
};

export type { Tokens };

export const users = {
  search: (q: string) => api<UserWithPresence[]>("GET", `/users/search?q=${encodeURIComponent(q)}`),
  profile: (id: string) => api<Profile>("GET", `/users/${id}`),
};

export const friends = {
  list: () => api<UserWithPresence[]>("GET", "/friends"),
  requests: () => api<FriendRequests>("GET", "/friends/requests"),
  request: (username: string) => api<{ status: string }>("POST", "/friends/requests", { username }),
  requestById: (userId: string) => api<{ status: string }>("POST", "/friends/requests", { userId }),
  accept: (id: string) => api<void>("POST", `/friends/requests/${id}/accept`),
  decline: (id: string) => api<void>("DELETE", `/friends/requests/${id}`),
  remove: (userId: string) => api<void>("DELETE", `/friends/${userId}`),
};

export const communities = {
  mine: () => api<CommunitySummary[]>("GET", "/communities"),
  discover: (q = "") => api<CommunitySummary[]>("GET", `/communities/discover?q=${encodeURIComponent(q)}`),
  create: (body: { name: string; description: string; isPublic: boolean }) =>
    api<CommunitySummary>("POST", "/communities", body),
  detail: (id: string) => api<CommunityDetail>("GET", `/communities/${id}`),
  join: (id: string) => api<CommunitySummary>("POST", `/communities/${id}/join`),
  leave: (id: string) => api<void>("POST", `/communities/${id}/leave`),
  remove: (id: string) => api<void>("DELETE", `/communities/${id}`),
  kick: (id: string, userId: string) => api<void>("DELETE", `/communities/${id}/members/${userId}`),
  invite: (id: string, userId: string) => api<InviteView>("POST", `/communities/${id}/invites`, { userId }),
};

export const rooms = {
  list: (query = "") =>
    api<RoomView[]>("GET", query.trim() ? `/rooms?q=${encodeURIComponent(query.trim())}` : "/rooms"),
  current: () => api<RoomSession | null>("GET", "/rooms/current"),
  create: (body: { name: string; communityId?: string | null; password?: string; maxPlayers?: number }) =>
    api<RoomSession>("POST", "/rooms", body),
  detail: (id: string) => api<RoomView>("GET", `/rooms/${id}`),
  join: (id: string, password?: string) => api<RoomSession>("POST", `/rooms/${id}/join`, { password }),
  session: (id: string) => api<RoomSession>("GET", `/rooms/${id}/session`),
  leave: (id: string) => api<void>("POST", `/rooms/${id}/leave`),
  close: (id: string) => api<void>("DELETE", `/rooms/${id}`),
  setStatus: (id: string, status: "open" | "in_game") => api<RoomView>("POST", `/rooms/${id}/status`, { status }),
  kick: (id: string, userId: string) => api<void>("DELETE", `/rooms/${id}/members/${userId}`),
  invite: (id: string, userId: string) => api<InviteView>("POST", `/rooms/${id}/invites`, { userId }),
};

export const invites = {
  pending: () => api<InviteView[]>("GET", "/invites"),
  accept: (id: string) =>
    api<{ kind: "community"; communityId: string } | { kind: "room"; session: RoomSession }>(
      "POST",
      `/invites/${id}/accept`,
    ),
  decline: (id: string) => api<void>("POST", `/invites/${id}/decline`),
};

// Chat lives on the helper edges: tried there first, the main server last.
async function onEdge<T>(call: (base: string | undefined) => Promise<T>): Promise<T> {
  try {
    return await call(edgeBase());
  } catch (error) {
    if (error instanceof ApiError && error.status !== 0) throw error;
    return call(undefined);
  }
}

export const chat = {
  history: (kind: ChannelKind, id: string, before?: string) =>
    onEdge((base) =>
      api<ChatMessage[]>(
        "GET",
        `/chat/${kind}/${id}/messages${before ? `?before=${encodeURIComponent(before)}` : ""}`,
        undefined,
        base,
      ),
    ),
  send: async (kind: ChannelKind, id: string, body: string) => {
    // One id for every attempt: if the socket attempt did reach the server
    // (it only timed out), the HTTP retry returns that same message.
    const requestId = crypto.randomUUID();
    try {
      return await realtime.sendChat(kind, id, body, requestId);
    } catch (error) {
      // A rejection by the server is final; a dead socket falls back to HTTP.
      if (error instanceof Error && !error.message.startsWith("socket")) throw error;
      return onEdge((base) => api<ChatMessage>("POST", `/chat/${kind}/${id}/messages`, { body, requestId }, base));
    }
  },
};

export const notifications = {
  list: () => api<AppNotification[]>("GET", "/notifications"),
  read: (id: string) => api<void>("POST", `/notifications/${id}/read`),
  readAll: () => api<void>("POST", "/notifications/read-all"),
};
