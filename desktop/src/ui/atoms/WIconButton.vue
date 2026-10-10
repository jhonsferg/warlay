<script setup lang="ts">
withDefaults(
  defineProps<{
    label: string;
    active?: boolean;
    badge?: number;
    size?: "sm" | "md";
    tone?: "default" | "danger";
  }>(),
  { active: false, badge: 0, size: "md", tone: "default" },
);
</script>

<template>
  <button
    type="button"
    class="w-icon-btn"
    :class="[`w-icon-btn--${size}`, `w-icon-btn--${tone}`, { 'w-icon-btn--active': active }]"
    :aria-label="label"
    :title="label"
  >
    <slot />
    <span v-if="badge > 0" class="w-icon-btn__badge">{{ badge > 99 ? "99+" : badge }}</span>
  </button>
</template>

<style scoped lang="scss">
.w-icon-btn {
  @include reset-button;
  @include interactive;
  position: relative;
  display: grid;
  place-items: center;
  border-radius: var(--r-sm);
  color: var(--text-2);

  &--sm {
    width: 28px;
    height: 28px;
    :deep(svg) {
      width: 16px;
      height: 16px;
    }
  }
  &--md {
    width: 36px;
    height: 36px;
    :deep(svg) {
      width: 19px;
      height: 19px;
    }
  }

  &:hover {
    background: var(--bg-4);
    color: var(--text-1);
  }
  &--active {
    background: var(--accent-soft);
    color: #c4b8ff;
  }
  &--danger:hover {
    background: var(--danger-soft);
    color: var(--danger);
  }

  &__badge {
    position: absolute;
    top: 1px;
    right: 1px;
    min-width: 17px;
    height: 17px;
    padding: 0 4px;
    border-radius: var(--r-full);
    background: var(--danger);
    color: #fff;
    font-size: 10px;
    font-weight: 700;
    line-height: 17px;
    text-align: center;
    box-shadow: 0 0 0 2px var(--bg-1);
  }
}
</style>
