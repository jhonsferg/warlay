<script setup lang="ts">
import { X } from "@lucide/vue";

withDefaults(defineProps<{ title: string; testId?: string }>(), { testId: undefined });
const emit = defineEmits<{ close: [] }>();
</script>

<template>
  <div class="modal" :data-testid="testId" @click.self="emit('close')">
    <div class="modal__panel" role="dialog" aria-modal="true" :aria-label="title">
      <div class="modal__header">
        <h2 class="modal__title">{{ title }}</h2>
        <button class="modal__close" type="button" aria-label="Cerrar" data-testid="modal-close" @click="emit('close')">
          <X :size="18" />
        </button>
      </div>
      <div class="modal__body">
        <slot />
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
// A sheet/panel, not a framed dialog box -- lighter scrim than before (a
// macOS sheet doesn't black out the window behind it), no border around
// the panel itself (separation comes from the shadow), and the panel
// scales in from slightly-smaller rather than only fading.
.modal {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.4);
  backdrop-filter: blur(8px);
  animation: modal-scrim-in 0.2s ease;

  &__panel {
    width: min(480px, calc(100vw - 48px));
    max-height: calc(100vh - 96px);
    display: flex;
    flex-direction: column;
    background: $bg-panel;
    border-radius: $radius-lg;
    box-shadow: $shadow-lg;
    overflow: hidden;
    animation: modal-panel-in 0.25s $ease-standard;
  }

  &__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: $spacing-md;
    padding: $spacing-md $spacing-lg;
    border-bottom: 1px solid $border-bronze;
  }

  &__title {
    margin: 0;
    font-size: $text-lg;
    font-weight: 600;
    border-bottom: none;
    padding-bottom: 0;
  }

  &__close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    background: rgba(255, 255, 255, 0.08);
    border: none;
    border-radius: 50%;
    color: $text-dim;
    cursor: pointer;
    transition: $transition;

    &:hover {
      background: rgba(255, 255, 255, 0.16);
      color: $text;
    }

    &:active {
      transform: scale(0.92);
    }
  }

  &__body {
    padding: $spacing-lg;
    overflow-y: auto;
  }
}

@keyframes modal-scrim-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

@keyframes modal-panel-in {
  from {
    opacity: 0;
    transform: scale(0.96);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}
</style>
