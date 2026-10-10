<script setup lang="ts">
// Centralizes the small pill-shaped status label used all over the app
// (connection status, room status, "con clave") -- previously every
// consumer wrote its own ad-hoc `class="badge" :class="someRawString"`,
// with the actual `.badge` CSS living in a global stylesheet none of them
// owned. One real component with its own scoped BEM styles instead.
//
// Six tones, six genuinely different hues -- a badge's color is meant to
// carry information (this is fine / this needs attention / this failed),
// so it can't all be gold at different opacities.
withDefaults(defineProps<{ tone?: "success" | "danger" | "amber" | "steel" | "gold" | "neutral" }>(), {
  tone: "neutral",
});
</script>

<template>
  <span class="badge" :class="`badge--${tone}`">
    <slot />
  </span>
</template>

<style scoped lang="scss">
// A tinted pill, no border -- Apple badges/tags read through a soft fill
// in the semantic color plus full-strength text in that same color, not a
// bordered chip.
.badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 9px;
  border-radius: $radius-full;
  font-size: $text-xs;
  font-weight: 600;
  background: rgba(255, 255, 255, 0.08);
  color: $text-dim;
  white-space: nowrap;

  &--success {
    background: $success-glow;
    color: $success;
  }

  &--danger {
    background: $danger-glow;
    color: $danger;
  }

  &--amber {
    background: $amber-glow;
    color: $amber;
  }

  &--steel {
    background: $steel-glow;
    color: $steel;
  }

  &--gold {
    background: $gold-dim;
    color: $gold-bright;
  }

  &--neutral {
    color: $text-dim;
  }
}
</style>
