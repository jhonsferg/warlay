<script setup lang="ts">
import { computed } from "vue";
import type { Presence, UserSummary } from "../../services/types";
import WAvatar from "../atoms/WAvatar.vue";
import { presenceLabel } from "../presence";

const props = withDefaults(
  defineProps<{ user: UserSummary; presence?: Presence | null; size?: "sm" | "md" | "lg"; subtitle?: string | null }>(),
  { presence: null, size: "md", subtitle: null },
);

const secondLine = computed(
  () => props.subtitle ?? (props.presence ? presenceLabel(props.presence.status) : `@${props.user.username}`),
);
</script>

<template>
  <div class="user-chip" :class="`user-chip--${size}`">
    <WAvatar :name="user.displayName" :color="user.avatarColor" :size="size" :status="presence?.status ?? null" />
    <div class="user-chip__text">
      <span class="user-chip__name">{{ user.displayName }}</span>
      <span class="user-chip__sub" :class="presence ? `user-chip__sub--${presence.status}` : ''">{{ secondLine }}</span>
    </div>
    <slot />
  </div>
</template>

<style scoped lang="scss">
.user-chip {
  display: flex;
  align-items: center;
  gap: var(--s-3);
  min-width: 0;

  &__text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  &__name {
    @include truncate;
    font-weight: 600;
    color: var(--text-1);
  }
  &__sub {
    @include truncate;
    font-size: var(--fs-sm);
    color: var(--text-3);

    &--in_game {
      color: var(--in-game);
    }
    &--in_room {
      color: var(--in-room);
    }
    &--online {
      color: var(--online);
    }
  }

  &--sm &__name {
    font-size: var(--fs-sm);
  }
  &--lg &__name {
    font-size: var(--fs-lg);
  }
}
</style>
