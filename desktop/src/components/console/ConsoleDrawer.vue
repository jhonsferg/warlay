<script setup lang="ts">
import { X } from "@lucide/vue";
import { useUiStore } from "../../store/ui";
import ConsoleOutput from "./ConsoleOutput.vue";

const uiStore = useUiStore();
</script>

<template>
  <div class="console-drawer" data-testid="console-drawer">
    <div class="console-drawer__header">
      <h2 class="console-drawer__title">Consola</h2>
      <button
        class="console-drawer__close"
        type="button"
        aria-label="Cerrar consola"
        data-testid="console-drawer-close"
        @click="uiStore.closeConsoleDrawer"
      >
        <X :size="16" />
      </button>
    </div>
    <ConsoleOutput />
  </div>
</template>

<style scoped lang="scss">
.console-drawer {
  display: flex;
  flex-direction: column;
  height: 320px;
  flex-shrink: 0;
  background: $bg-stone;
  backdrop-filter: $glass-blur;
  border-top: 1px solid $border-bronze-light;
  box-shadow: 0 -8px 24px -6px rgba(0, 0, 0, 0.4);

  &__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
    padding: $spacing-sm $spacing-lg;
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
}
</style>
