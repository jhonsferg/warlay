<script setup lang="ts">
import { computed } from "vue";
import type { LucideIcon } from "@lucide/vue";
import { playHover, playClick } from "../../utils/audio";

const props = withDefaults(
  defineProps<{
    variant?: string;
    disabled?: boolean;
    full?: boolean;
    icon?: LucideIcon;
    iconSize?: number;
    testId?: string;
    ariaLabel?: string;
  }>(),
  {
    variant: "",
    disabled: false,
    full: false,
    icon: undefined,
    iconSize: 16,
    testId: undefined,
    ariaLabel: undefined,
  },
);

// `variant` stays a space-separated string at every call site (e.g.
// "danger tiny") for a terse API -- this just maps each word onto a real
// BEM modifier class (btn--danger, btn--tiny) rather than emitting bare
// "danger"/"tiny" classes with no namespace. `full` defaults to true (every
// existing call site already relied on the button always filling its
// container) and is folded into the same modifier-class list rather than a
// separate `:class` binding.
const modifierClasses = computed(() => {
  const classes = props.variant
    .split(/\s+/)
    .filter(Boolean)
    .map((word) => `btn--${word}`);
  if (props.full) classes.push("btn--full");
  return classes;
});
</script>

<template>
  <button
    class="btn"
    :class="modifierClasses"
    type="button"
    :disabled="disabled"
    :data-testid="testId"
    :aria-label="ariaLabel"
    @mouseenter="!disabled && playHover()"
    @mousedown="!disabled && playClick()"
  >
    <component :is="icon" v-if="icon" class="btn__icon" :size="iconSize" />
    <slot />
  </button>
</template>

<style scoped lang="scss">
// Apple's own button taxonomy, minus the ones no call site actually needs
// yet (a borderless "plain" style, a tinted-not-solid variant) -- flat
// fills, no border, no bevel; the one interaction every native macOS
// control shares is compressing slightly on press instead of (or besides)
// changing color.
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 9px 18px;
  background: $bg-parchment;
  color: $text;
  border: none;
  border-radius: $radius-md;
  cursor: pointer;
  font-family: $font-body;
  font-weight: 500;
  font-size: $text-md;
  transition: $transition;

  &:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.14);
  }

  &:active:not(:disabled) {
    transform: scale(0.97);
  }

  &:focus-visible {
    outline: none;
    box-shadow: $focus-ring;
  }

  &:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  // The one filled/solid variant, reserved for the primary action on a
  // screen -- same restraint rule as the accent color itself.
  &--primary {
    background: $gold;
    color: #fff;

    &:hover:not(:disabled) {
      background: $gold-bright;
    }
  }

  // Quieter than the base button -- used for "Cancelar"/secondary actions
  // sitting next to a primary one.
  &--secondary {
    background: rgba(255, 255, 255, 0.05);
    color: $text-dim;

    &:hover:not(:disabled) {
      background: rgba(255, 255, 255, 0.1);
      color: $text;
    }
  }

  // Destructive action, Apple-style: text in the danger color with no
  // fill at rest, a soft tinted fill only on hover -- not a permanently
  // red button.
  &--danger {
    background: transparent;
    color: $danger;

    &:hover:not(:disabled) {
      background: $danger-glow;
    }
  }

  &--tiny {
    padding: 5px 12px;
    font-size: $text-sm;
  }

  &__icon {
    flex-shrink: 0;
  }

  &--full {
    width: 100%;
  }
}
</style>
