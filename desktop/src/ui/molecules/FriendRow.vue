<script setup lang="ts">
import type { UserWithPresence } from "../../services/types";
import UserChip from "./UserChip.vue";

defineProps<{ friend: UserWithPresence; compact?: boolean }>();
</script>

<template>
  <div
    class="friend-row"
    :class="{ 'friend-row--compact': compact, 'friend-row--offline': friend.presence.status === 'offline' }"
    :data-friend="friend.username"
  >
    <UserChip :user="friend" :presence="friend.presence" :size="compact ? 'sm' : 'md'" />
    <div v-if="$slots.actions" class="friend-row__actions"><slot name="actions" /></div>
  </div>
</template>

<style scoped lang="scss">
.friend-row {
  @include interactive;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s-3);
  padding: var(--s-2) var(--s-3);
  border-radius: var(--r-md);

  &:hover {
    background: var(--bg-4);
  }
  &--offline {
    opacity: 0.55;
  }
  &--offline:hover {
    opacity: 1;
  }
  &--compact {
    padding: 6px var(--s-2);
  }

  &__actions {
    display: flex;
    gap: 2px;
    opacity: 0.4;
    transition: opacity var(--t-fast);
  }
  &:hover &__actions,
  &:focus-within &__actions {
    opacity: 1;
  }
}
</style>
