<script setup lang="ts">
import { Copy, Eraser } from "@lucide/vue";
import { nextTick, ref, watch } from "vue";
import { useConsoleStore } from "../../store/console";
import BaseButton from "../base/BaseButton.vue";

const consoleStore = useConsoleStore();
const outputEl = ref<HTMLDivElement | null>(null);
const autoScroll = ref(true);
const copied = ref(false);

watch(
  () => consoleStore.consoleLines.length,
  async () => {
    if (!autoScroll.value) return;
    await nextTick();
    if (outputEl.value) outputEl.value.scrollTop = outputEl.value.scrollHeight;
  },
);

function onScroll(): void {
  if (!outputEl.value) return;
  const distanceFromBottom = outputEl.value.scrollHeight - outputEl.value.scrollTop - outputEl.value.clientHeight;
  autoScroll.value = distanceFromBottom < 40;
}

async function copyAll(): Promise<void> {
  await navigator.clipboard.writeText(consoleStore.consoleLines.join("\n"));
  copied.value = true;
  setTimeout(() => (copied.value = false), 1200);
}

function clear(): void {
  consoleStore.clearConsole();
}
</script>

<template>
  <div class="console-panel">
    <div class="console-panel__actions">
      <BaseButton variant="secondary tiny" :icon="Copy" :icon-size="14" test-id="console-copy" @click="copyAll">
        {{ copied ? "Copiado" : "Copiar todo" }}
      </BaseButton>
      <BaseButton variant="danger tiny" :icon="Eraser" :icon-size="14" test-id="console-clear" @click="clear">
        Limpiar
      </BaseButton>
    </div>
    <p class="console-panel__hint">
      Registro en vivo de lo que hace la app y su conversación con los servidores — sirve para confirmar que una
      conexión, sala o retransmisión de partida realmente está funcionando. También queda guardado en
      <code>%APPDATA%\com.warlay.lanlauncher\logs\</code>.
    </p>
    <div
      ref="outputEl"
      class="console-output"
      role="log"
      aria-live="polite"
      data-testid="console-output"
      @scroll="onScroll"
    >
      <p v-if="consoleStore.consoleLines.length === 0" class="empty">Sin actividad todavía.</p>
      <div v-for="(line, index) in consoleStore.consoleLines" :key="index" class="console-output__line">{{ line }}</div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.console-panel {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  padding: $spacing-md $spacing-lg;
  gap: $spacing-sm;

  &__actions {
    display: flex;
    gap: $spacing-sm;
  }

  &__hint {
    margin: 0;
    font-size: $text-xs;
    color: $text-muted;

    code {
      color: $text-dim;
      background: rgba(0, 0, 0, 0.3);
      padding: 1px 4px;
      border-radius: $radius-sm;
    }
  }
}

.console-output {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: $spacing-sm $spacing-md;
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid $border-bronze;
  border-radius: $radius-md;
  font-family: "Consolas", "Cascadia Mono", monospace;
  font-size: $text-xs;
  color: $text-dim;

  &__line {
    white-space: pre-wrap;
    word-break: break-word;
    padding: 1px 0;
  }
}
</style>
