<script setup lang="ts">
import type { Component } from "vue";
import { playHover, playClick } from "../../utils/audio";

withDefaults(defineProps<{ icon: Component; label: string; active?: boolean; testId?: string }>(), {
  active: false,
  testId: undefined,
});
const emit = defineEmits<{ click: [] }>();

function onClick() {
  playClick();
  emit("click");
}
</script>

<template>
  <button
    class="rail-icon-btn"
    :class="{ 'rail-icon-btn--active': active }"
    type="button"
    :aria-current="active"
    :data-testid="testId"
    @mouseenter="playHover()"
    @click="onClick"
  >
    <component :is="icon" class="rail-icon-btn__icon" :size="18" />
    <span>{{ label }}</span>
  </button>
</template>

<style scoped lang="scss">
.rail-icon-btn {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 12px;
  background: transparent;
  border: none;
  border-radius: $radius-md;
  color: $text-dim;
  font-family: $font-body;
  font-size: $text-sm;
  cursor: pointer;
  transition: $transition;

  &:hover {
    background: rgba(255, 255, 255, 0.08);
    color: $text;
  }

  &:focus-visible {
    outline: none;
    box-shadow: $focus-ring;
  }

  // A macOS source-list selected row is a full accent-tinted pill, not a
  // bordered outline in a separate "neutral" color -- unlike the old
  // theme, nav selection now shares the same accent as a primary button.
  &--active {
    background: $gold-dim;
    color: $gold-bright;
  }

  &__icon {
    flex-shrink: 0;
  }

  span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
}
</style>
