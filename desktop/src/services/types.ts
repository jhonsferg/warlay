// Shapes of the main server's REST API and realtime events (camelCase, as
// serialized by crates/social).

export type PresenceStatus = "offline" | "online" | "in_room" | "in_game";

export interface Presence {
  status: PresenceStatus;
  roomId: string | null;
}

export interface UserSummary {
  id: string;
  username: string;
  displayName: string;
  avatarColor: string;
}

export interface UserWithPresence extends UserSummary {
  presence: Presence;
}

export interface Me extends UserSummary {
  email: string;
  bio: string;
  createdAt: string;
}

export interface Profile extends UserWithPresence {
  bio: string;
  memberSince: string;
  isFriend: boolean;
}

export interface Tokens {
  accessToken: string;
  refreshToken: string;
  expiresIn: number;
}

export interface AuthResponse {
  user: Me;
  tokens: Tokens;
}

export interface FriendRequest {
  id: string;
  from: UserSummary;
  to: UserSummary;
  createdAt: string;
}

export interface FriendRequests {
  incoming: FriendRequest[];
  outgoing: FriendRequest[];
}

export type CommunityRole = "owner" | "admin" | "member";

export interface CommunitySummary {
  id: string;
  name: string;
  slug: string;
  description: string;
  accentColor: string;
  isPublic: boolean;
  ownerId: string;
  memberCount: number;
  myRole: CommunityRole | null;
  createdAt: string;
}

export interface CommunityMember extends UserWithPresence {
  role: CommunityRole;
}

export interface CommunityDetail extends CommunitySummary {
  members: CommunityMember[];
  rooms: RoomView[];
}

export type RoomStatus = "open" | "in_game" | "closed";

export interface RoomView {
  id: string;
  name: string;
  communityId: string | null;
  communityName: string | null;
  host: UserSummary;
  hasPassword: boolean;
  maxPlayers: number;
  gamePort: number;
  status: RoomStatus;
  members: UserSummary[];
  createdAt: string;
  /** When the server read this state (µs since the epoch); updates older
   * than the state already held are stale and dropped. */
  snapshotAt?: number;
}

export interface HelperInfo {
  id: string;
  name: string;
  region: string;
  rendezvousAddr: string;
  relayAddr: string;
  edgeUrl?: string;
}

export interface RoomSession {
  room: RoomView;
  helper: HelperInfo;
  ticket: string;
  ticketExpiresUnix: number;
  isHost: boolean;
  userId: string;
}

export interface InviteView {
  id: string;
  kind: "community" | "room";
  targetId: string;
  targetName: string;
  inviter: UserSummary;
  createdAt: string;
}

export interface ChatMessage {
  id: string;
  channel: string;
  sender: UserSummary;
  body: string;
  createdAt: string;
}

/** How far a conversation partner has received and read what we sent them. */
export interface ReceiptCursors {
  deliveredAt: string;
  readAt: string;
}

/** What the sender sees under a direct message. */
export type MessageStatus = "sent" | "delivered" | "read";

/** A friend whose direct messages we haven't read. */
export interface UnreadSummary {
  peerId: string;
  count: number;
  /** Time of the newest unread message. */
  latestAt: string;
}

export interface AppNotification {
  id: string;
  kind: string;
  payload: Record<string, unknown>;
  read: boolean;
  createdAt: string;
}

export type ChannelKind = "dm" | "room" | "community";

export type RealtimeEvent =
  | { type: "hello" }
  | { type: "presence"; userId: string; presence: Presence }
  | { type: "friendRequest"; request: FriendRequest | { id: string; removed: true } }
  | { type: "friendAdded"; friend: UserWithPresence }
  | { type: "friendRemoved"; userId: string }
  | { type: "notification"; notification: AppNotification }
  | { type: "message"; message: ChatMessage }
  | { type: "messageDeleted"; channel: string; messageId: string }
  | { type: "chatReceipt"; channel: string; userId: string; receipt: ReceiptCursors }
  | { type: "roomUpdated"; room: RoomView }
  | { type: "roomClosed"; roomId: string }
  | { type: "roomHelperChanged"; roomId: string; helper: HelperInfo }
  | { type: "invite"; invite: InviteView }
  | { type: "communityUpdated"; communityId: string }
  | { type: "sessionRevoked" };

export interface ApiErrorBody {
  error: string;
  message: string;
}
