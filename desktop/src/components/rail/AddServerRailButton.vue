<script setup lang="ts">
import { Plus } from "@lucide/vue";
const emit = defineEmits<{ click: [] }>();
</script>

<template>
  <button class="add-server-btn" type="button" data-testid="rail-add-server" @click="emit('click')">
    <component :is="Plus" class="add-server-btn__icon" :size="18" />
    <span>Registrar servidor</span>
  </button>
</template>

<style scoped lang="scss">
// A dashed border reads as a placeholder/drop-zone convention, not
// something Apple's own "create new" rows use -- same flat row treatment
// as the rest of the sidebar instead, with the accent tint only appearing
// on hover.
.add-server-btn {
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
  display: flex;
  align-items: center;
  gap: 10px;

  &:hover {
    color: $gold-bright;
    background: $gold-dim;
  }

  &:focus-visible {
    outline: none;
    box-shadow: $focus-ring;
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
