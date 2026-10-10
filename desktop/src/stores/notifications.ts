import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { invites as invitesApi, notifications as api } from "../services/api";
import type { AppNotification, InviteView } from "../services/types";

export const useNotifications = defineStore("notifications", () => {
  const items = ref<AppNotification[]>([]);
  const invites = ref<InviteView[]>([]);

  const unread = computed(() => items.value.filter((n) => !n.read).length);

  async function load() {
    const [list, pending] = await Promise.all([api.list(), invitesApi.pending()]);
    items.value = list;
    invites.value = pending;
  }

  async function markAllRead() {
    if (unread.value === 0) return;
    await api.readAll();
    items.value = items.value.map((n) => ({ ...n, read: true }));
  }

  async function acceptInvite(id: string) {
    const result = await invitesApi.accept(id);
    invites.value = invites.value.filter((i) => i.id !== id);
    return result;
  }

  async function declineInvite(id: string) {
    await invitesApi.decline(id);
    invites.value = invites.value.filter((i) => i.id !== id);
  }

  function applyNotification(n: AppNotification) {
    items.value = [n, ...items.value.filter((x) => x.id !== n.id)];
  }

  function applyInvite(invite: InviteView) {
    invites.value = [invite, ...invites.value.filter((i) => i.id !== invite.id)];
  }

  function reset() {
    items.value = [];
    invites.value = [];
  }

  return {
    items,
    invites,
    unread,
    load,
    markAllRead,
    acceptInvite,
    declineInvite,
    applyNotification,
    applyInvite,
    reset,
  };
});
