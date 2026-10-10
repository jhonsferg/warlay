<script setup lang="ts">
import WSpinner from "./WSpinner.vue";

withDefaults(
  defineProps<{
    variant?: "primary" | "secondary" | "ghost" | "danger" | "success";
    size?: "sm" | "md" | "lg";
    loading?: boolean;
    disabled?: boolean;
    block?: boolean;
    type?: "button" | "submit";
  }>(),
  { variant: "primary", size: "md", loading: false, disabled: false, block: false, type: "button" },
);
</script>

<template>
  <button
    :type="type"
    class="w-btn"
    :class="[`w-btn--${variant}`, `w-btn--${size}`, { 'w-btn--block': block, 'w-btn--loading': loading }]"
    :disabled="disabled || loading"
    :aria-busy="loading || undefined"
  >
    <WSpinner v-if="loading" class="w-btn__spinner" :size="size === 'sm' ? 12 : 16" />
    <span class="w-btn__icon" v-if="$slots.icon && !loading"><slot name="icon" /></span>
    <span class="w-btn__label"><slot /></span>
  </button>
</template>

<style scoped lang="scss">
.w-btn {
  @include reset-button;
  @include interactive;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--s-2);
  border-radius: var(--r-sm);
  font-weight: 600;
  white-space: nowrap;
  position: relative;
  user-select: none;

  &:active:not(:disabled) {
    transform: translateY(1px);
  }

  &:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  &--sm {
    height: 30px;
    padding: 0 var(--s-3);
    font-size: var(--fs-sm);
  }
  &--md {
    height: 38px;
    padding: 0 var(--s-4);
    font-size: var(--fs-md);
  }
  &--lg {
    height: 46px;
    padding: 0 var(--s-6);
    font-size: var(--fs-lg);
  }
  &--block {
    width: 100%;
  }

  &--primary {
    background: var(--accent);
    color: var(--text-on-accent);
    box-shadow: 0 1px 0 rgba(255, 255, 255, 0.15) inset;
    &:hover:not(:disabled) {
      background: var(--accent-hover);
      box-shadow: var(--glow-accent);
    }
    &:active:not(:disabled) {
      background: var(--accent-press);
    }
  }
  &--secondary {
    background: var(--bg-4);
    color: var(--text-1);
    border: 1px solid var(--line-2);
    &:hover:not(:disabled) {
      background: var(--bg-5);
      border-color: var(--line-3);
    }
  }
  &--ghost {
    background: transparent;
    color: var(--text-2);
    &:hover:not(:disabled) {
      background: var(--bg-4);
      color: var(--text-1);
    }
  }
  &--danger {
    background: var(--danger-soft);
    color: var(--danger);
    border: 1px solid rgba(244, 63, 94, 0.3);
    &:hover:not(:disabled) {
      background: var(--danger);
      color: #fff;
    }
  }
  &--success {
    background: var(--success);
    color: #052e1f;
    &:hover:not(:disabled) {
      filter: brightness(1.08);
      box-shadow: 0 8px 24px rgba(52, 211, 153, 0.25);
    }
  }

  &__icon {
    display: inline-flex;
    :deep(svg) {
      width: 1.1em;
      height: 1.1em;
    }
  }
}
</style>
