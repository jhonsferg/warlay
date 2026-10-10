<script setup lang="ts">
import { LogOut, Settings } from "@lucide/vue";
import { computed } from "vue";
import { useRouter } from "vue-router";
import { useRooms } from "../../stores/rooms";
import { useSession } from "../../stores/session";
import WIconButton from "../atoms/WIconButton.vue";
import UserChip from "../molecules/UserChip.vue";

const session = useSession();
const rooms = useRooms();
const router = useRouter();

const presence = computed(() => ({
  status: rooms.current
    ? rooms.current.status === "in_game"
      ? ("in_game" as const)
      : ("in_room" as const)
    : ("online" as const),
  roomId: rooms.current?.id ?? null,
}));

async function logout() {
  await rooms.leave().catch(() => {});
  await session.logout();
  void router.push("/login");
}
</script>

<template>
  <div v-if="session.me" class="user-panel" data-testid="user-panel">
    <UserChip :user="session.me" :presence="presence" size="sm" :subtitle="`@${session.me.username}`" />
    <div class="user-panel__actions">
      <WIconButton label="Ajustes" size="sm" @click="router.push('/settings')"><Settings /></WIconButton>
      <WIconButton label="Cerrar sesión" size="sm" tone="danger" @click="logout"><LogOut /></WIconButton>
    </div>
  </div>
</template>

<style scoped lang="scss">
.user-panel {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s-2);
  padding: var(--s-2) var(--s-3);
  background: var(--bg-1);
  border-top: 1px solid var(--line);
  --dot-ring: var(--bg-1);

  :deep(.user-chip) {
    flex: 1;
  }
  &__actions {
    display: flex;
  }
}
</style>
