import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { blocks as api } from "../services/api";
import type { BlockedUser } from "../services/types";

/** The players the signed-in user has blocked. Their messages are hidden in
 * rooms and communities; the server keeps them out of everything private. */
export const useBlocks = defineStore("blocks", () => {
  const list = ref<BlockedUser[]>([]);
  const ids = computed(() => new Set(list.value.map((b) => b.id)));

  async function load() {
    list.value = await api.list();
  }

  /** Blocks `userId`; the friendship, if any, ends on the server. */
  async function block(userId: string) {
    await api.block(userId);
    await load();
  }

  async function unblock(userId: string) {
    await api.unblock(userId);
    list.value = list.value.filter((b) => b.id !== userId);
  }

  function isBlocked(userId: string) {
    return ids.value.has(userId);
  }

  function reset() {
    list.value = [];
  }

  return { list, ids, load, block, unblock, isBlocked, reset };
});
