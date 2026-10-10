<script setup lang="ts">
import { computed } from "vue";
import type { PresenceStatus } from "../../services/types";
import WStatusDot from "./WStatusDot.vue";

const props = withDefaults(
  defineProps<{
    name: string;
    color?: string;
    size?: "xs" | "sm" | "md" | "lg" | "xl";
    status?: PresenceStatus | null;
    shape?: "circle" | "squircle";
  }>(),
  { color: "#7b61ff", size: "md", status: null, shape: "circle" },
);

const initials = computed(() => {
  const words = props.name
    .trim()
    .split(/[\s_\-.]+/)
    .filter(Boolean);
  const letters = words.length > 1 ? words[0][0] + words[1][0] : (words[0] ?? "?").slice(0, 2);
  // Small avatars overlap in stacks: one letter stays legible.
  return (props.size === "xs" || props.size === "sm" ? letters.slice(0, 1) : letters).toUpperCase();
});
</script>

<template>
  <span class="w-avatar" :class="[`w-avatar--${size}`, `w-avatar--${shape}`]" :aria-label="name" role="img">
    <span class="w-avatar__face" :style="{ '--avatar-color': color }">{{ initials }}</span>
    <WStatusDot
      v-if="status"
      class="w-avatar__status"
      :status="status"
      :size="size === 'xs' || size === 'sm' ? 'sm' : 'md'"
    />
  </span>
</template>

<style scoped lang="scss">
.w-avatar {
  position: relative;
  display: inline-flex;
  flex-shrink: 0;

  &__face {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    font-weight: 700;
    color: #fff;
    letter-spacing: 0.02em;
    background: radial-gradient(120% 120% at 20% 10%, rgba(255, 255, 255, 0.28), transparent 55%), var(--avatar-color);
    border-radius: 50%;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.12);
    user-select: none;
  }

  &--squircle &__face {
    border-radius: 30%;
  }

  &--xs {
    width: 20px;
    height: 20px;
    font-size: 8px;
  }
  &--sm {
    width: 28px;
    height: 28px;
    font-size: 11px;
  }
  &--md {
    width: 36px;
    height: 36px;
    font-size: 13px;
  }
  &--lg {
    width: 48px;
    height: 48px;
    font-size: 17px;
  }
  &--xl {
    width: 84px;
    height: 84px;
    font-size: 28px;
  }

  &__status {
    position: absolute;
    right: -2px;
    bottom: -2px;
  }
}
</style>
