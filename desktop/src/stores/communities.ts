import { defineStore } from "pinia";
import { ref } from "vue";
import { communities as api } from "../services/api";
import type { CommunityDetail, CommunitySummary } from "../services/types";

export const useCommunities = defineStore("communities", () => {
  const mine = ref<CommunitySummary[]>([]);
  const details = ref<Record<string, CommunityDetail>>({});
  const discovered = ref<CommunitySummary[]>([]);

  async function loadMine() {
    mine.value = await api.mine();
  }

  async function loadDetail(id: string) {
    details.value[id] = await api.detail(id);
    return details.value[id];
  }

  async function discover(q = "") {
    discovered.value = await api.discover(q);
  }

  async function create(body: { name: string; description: string; isPublic: boolean }) {
    const created = await api.create(body);
    mine.value = [...mine.value, created].sort((a, b) => a.name.localeCompare(b.name));
    return created;
  }

  async function join(id: string) {
    const joined = await api.join(id);
    await loadMine();
    discovered.value = discovered.value.map((c) => (c.id === id ? joined : c));
    return joined;
  }

  async function leave(id: string) {
    await api.leave(id);
    mine.value = mine.value.filter((c) => c.id !== id);
    delete details.value[id];
  }

  async function remove(id: string) {
    await api.remove(id);
    mine.value = mine.value.filter((c) => c.id !== id);
    delete details.value[id];
  }

  /** Realtime: membership or rooms changed somewhere in this community. */
  async function refresh(id: string) {
    await loadMine();
    if (!mine.value.some((c) => c.id === id)) {
      delete details.value[id];
      return;
    }
    if (details.value[id]) await loadDetail(id);
  }

  const pendingRefresh = new Map<string, ReturnType<typeof setTimeout>>();

  /** `refresh` on a `communityUpdated` event: every member of the community
   * gets it at once, so wait a random moment (and fold repeats) instead of
   * all of them refetching in the same instant. */
  function refreshSoon(id: string) {
    if (pendingRefresh.has(id)) return;
    const timer = setTimeout(
      () => {
        pendingRefresh.delete(id);
        void refresh(id).catch(() => {});
      },
      300 + Math.random() * 2000,
    );
    pendingRefresh.set(id, timer);
  }

  function reset() {
    pendingRefresh.forEach(clearTimeout);
    pendingRefresh.clear();
    mine.value = [];
    details.value = {};
    discovered.value = [];
  }

  return {
    mine,
    details,
    discovered,
    loadMine,
    loadDetail,
    discover,
    create,
    join,
    leave,
    remove,
    refresh,
    refreshSoon,
    reset,
  };
});
