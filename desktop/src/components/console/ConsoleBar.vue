<script setup lang="ts">
import { ChevronDown, ChevronUp, Terminal } from "@lucide/vue";
import { computed } from "vue";
import { useConsoleStore } from "../../store/console";
import { useUiStore } from "../../store/ui";

const consoleStore = useConsoleStore();
const uiStore = useUiStore();
const lineCount = computed(() => consoleStore.consoleLines.length);
</script>

<template>
  <div class="console-bar" data-testid="console-bar">
    <button
      class="console-bar__toggle"
      type="button"
      :aria-expanded="uiStore.consoleDrawerOpen"
      data-testid="console-bar-toggle"
      @click="uiStore.toggleConsoleDrawer"
    >
      <Terminal :size="16" />
      <span>Consola</span>
      <span v-if="lineCount > 0" class="console-bar__count">{{ lineCount }}</span>
      <ChevronDown v-if="uiStore.consoleDrawerOpen" :size="16" />
      <ChevronUp v-else :size="16" />
    </button>
  </div>
</template>

<style scoped lang="scss">
.console-bar {
  flex-shrink: 0;
  border-top: 1px solid $border-bronze;

  &__toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 16px;
    background: $bg-stone;
    backdrop-filter: $glass-blur;
    border: none;
    color: $text-dim;
    font-family: $font-body;
    font-size: $text-sm;
    cursor: pointer;
    transition: $transition;

    &:hover {
      color: $text;
    }
  }

  &__count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    padding: 0 5px;
    border-radius: $radius-full;
    background: $gold-dim;
    color: $gold-bright;
    font-size: $text-xs;
    font-weight: 600;
  }
}
</style>
