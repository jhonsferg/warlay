<script setup lang="ts">
withDefaults(
  defineProps<{
    tone?: "neutral" | "accent" | "success" | "warning" | "danger" | "info" | "game";
    dot?: boolean;
  }>(),
  { tone: "neutral", dot: false },
);
</script>

<template>
  <span class="w-badge" :class="`w-badge--${tone}`">
    <span v-if="dot" class="w-badge__dot" />
    <slot />
  </span>
</template>

<style scoped lang="scss">
.w-badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 22px;
  padding: 0 9px;
  border-radius: var(--r-full);
  font-size: var(--fs-xs);
  font-weight: 600;
  letter-spacing: 0.02em;
  white-space: nowrap;
  border: 1px solid transparent;

  &__dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }

  &--neutral {
    background: var(--bg-4);
    color: var(--text-2);
    border-color: var(--line-2);
  }
  &--accent {
    background: var(--accent-soft);
    color: #b9aaff;
  }
  &--success {
    background: var(--success-soft);
    color: var(--success);
  }
  &--warning {
    background: var(--warning-soft);
    color: var(--warning);
  }
  &--danger {
    background: var(--danger-soft);
    color: var(--danger);
  }
  &--info {
    background: var(--info-soft);
    color: var(--info);
  }
  &--game {
    background: rgba(244, 114, 182, 0.14);
    color: var(--in-game);
  }

  :deep(svg) {
    width: 12px;
    height: 12px;
  }
}
</style>
