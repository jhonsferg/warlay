import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { friends as friendsApi, users } from "../services/api";
import type { FriendRequest, Presence, UserSummary, UserWithPresence } from "../services/types";
import { PRESENCE_ORDER } from "../ui/presence";

/** Friends, friend requests and live presence. */
export const useSocial = defineStore("social", () => {
  const friends = ref<UserWithPresence[]>([]);
  const incoming = ref<FriendRequest[]>([]);
  const outgoing = ref<FriendRequest[]>([]);
  const loaded = ref(false);

  const sortedFriends = computed(() =>
    [...friends.value].sort(
      (a, b) =>
        PRESENCE_ORDER.indexOf(a.presence.status) - PRESENCE_ORDER.indexOf(b.presence.status) ||
        a.displayName.localeCompare(b.displayName),
    ),
  );
  const onlineCount = computed(() => friends.value.filter((f) => f.presence.status !== "offline").length);

  async function load() {
    const [list, requests] = await Promise.all([friendsApi.list(), friendsApi.requests()]);
    friends.value = list;
    incoming.value = requests.incoming;
    outgoing.value = requests.outgoing;
    loaded.value = true;
  }

  function friend(id: string): UserWithPresence | undefined {
    return friends.value.find((f) => f.id === id);
  }

  async function sendRequest(username: string) {
    const res = await friendsApi.request(username.trim());
    await load();
    return res.status;
  }

  async function accept(id: string) {
    await friendsApi.accept(id);
    await load();
  }

  async function decline(id: string) {
    await friendsApi.decline(id);
    incoming.value = incoming.value.filter((r) => r.id !== id);
    outgoing.value = outgoing.value.filter((r) => r.id !== id);
  }

  async function remove(userId: string) {
    await friendsApi.remove(userId);
    friends.value = friends.value.filter((f) => f.id !== userId);
  }

  const search = (q: string) => users.search(q);

  // Realtime handlers.
  function applyPresence(userId: string, presence: Presence) {
    const f = friend(userId);
    if (f) f.presence = presence;
  }
  function applyFriendAdded(user: UserWithPresence) {
    friends.value = [...friends.value.filter((f) => f.id !== user.id), user];
  }
  function applyFriendRemoved(userId: string) {
    friends.value = friends.value.filter((f) => f.id !== userId);
  }
  function applyRequest(request: FriendRequest | { id: string; removed: true }, myId: string) {
    incoming.value = incoming.value.filter((r) => r.id !== request.id);
    outgoing.value = outgoing.value.filter((r) => r.id !== request.id);
    if ("removed" in request) return;
    if (request.to.id === myId) incoming.value.unshift(request);
    else outgoing.value.unshift(request);
  }

  function reset() {
    friends.value = [];
    incoming.value = [];
    outgoing.value = [];
    loaded.value = false;
  }

  /** Presence for any user we know of (friends only, for now). */
  function presenceOf(user: UserSummary): Presence | null {
    return friend(user.id)?.presence ?? null;
  }

  return {
    friends,
    sortedFriends,
    onlineCount,
    incoming,
    outgoing,
    loaded,
    load,
    friend,
    sendRequest,
    accept,
    decline,
    remove,
    search,
    applyPresence,
    applyFriendAdded,
    applyFriendRemoved,
    applyRequest,
    presenceOf,
    reset,
  };
});
