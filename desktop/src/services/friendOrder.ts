/**
 * How the friends list is ordered and searched. Pure, so it is unit tested.
 */
const PRESENCE_RANK = { in_game: 0, in_room: 1, online: 2, offline: 3 } as const;

interface FriendLike {
  displayName: string;
  username: string;
  presence: { status: keyof typeof PRESENCE_RANK; lastSeenAt?: string | null };
}

/** Lower-case and accent-free, so "angel" finds "Ángel". */
const fold = (text: string) =>
  text
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase();

const seenAt = (f: FriendLike) => {
  const t = f.presence.lastSeenAt ? Date.parse(f.presence.lastSeenAt) : NaN;
  return Number.isNaN(t) ? 0 : t;
};

/**
 * Around first (in game, in a room, online); then the offline ones, the most
 * recently seen on top and those never seen last; ties by name.
 */
export function compareFriends(a: FriendLike, b: FriendLike): number {
  return (
    PRESENCE_RANK[a.presence.status] - PRESENCE_RANK[b.presence.status] ||
    (a.presence.status === "offline" ? seenAt(b) - seenAt(a) : 0) ||
    fold(a.displayName).localeCompare(fold(b.displayName))
  );
}

export function matchesQuery(friend: FriendLike, query: string): boolean {
  const q = fold(query.trim());
  if (!q) return true;
  return fold(friend.displayName).includes(q) || fold(friend.username).includes(q);
}
