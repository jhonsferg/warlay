<script setup lang="ts">
withDefaults(defineProps<{ testId?: string; ariaLabel?: string }>(), {
  testId: undefined,
  ariaLabel: undefined,
});
const model = defineModel<boolean>({ default: false });
</script>

<template>
  <input v-model="model" class="base-checkbox" type="checkbox" :data-testid="testId" :aria-label="ariaLabel" />
</template>

<style scoped lang="scss">
// Every current call site is a standalone on/off preference (silenciar
// sonidos, activar chat de voz, usar esta subred/peer en el túnel), not a
// multi-select list needing a square checkbox -- so this renders as an
// iOS/macOS-style switch instead. Still a real <input type="checkbox">
// under the hood (appearance: none + a drawn track/thumb via ::before) so
// keyboard toggling, :checked state, and aria all keep working with no
// script changes.
.base-checkbox {
  appearance: none;
  position: relative;
  width: 36px;
  height: 20px;
  flex-shrink: 0;
  border-radius: $radius-full;
  background: $bg-parchment;
  border: 1px solid $border-bronze;
  cursor: pointer;
  transition: $transition;

  // The thumb is positioned relative to the track's *padding* box (per
  // CSS's own absolute-positioning rules) -- the 1px border already eats
  // into the 20px track height before this offset is even applied. At
  // top/left: 2px, that left a 3px gap from the track's actual outer
  // edge on top/left but only 1px on the bottom (2px inset + 0, since
  // 18px padding-box - 2px top - 16px thumb = 0) -- asymmetric, and the
  // false-state thumb read as floating with visible daylight around it
  // instead of sitting snug in the track corner. 1px here (on an 18px
  // padding-box) centers the 16px thumb exactly, and combined with the
  // 1px border that's a consistent, tight 2px gap on every side.
  &::before {
    content: "";
    position: absolute;
    top: 1px;
    left: 1px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
    transition: $transition;
  }

  &:checked {
    background: $gold;
    border-color: $gold;

    &::before {
      transform: translateX(16px);
    }
  }

  &:focus-visible {
    outline: none;
    box-shadow: $focus-ring;
  }
}
</style>
