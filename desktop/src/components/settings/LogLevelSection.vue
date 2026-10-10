<script setup lang="ts">
import { computed } from "vue";
import { useSettingsStore } from "../../store/settings";
import type { LogLevel } from "../../types";
import BaseNumberInput from "../base/BaseNumberInput.vue";

const settingsStore = useSettingsStore();
const LEVELS: LogLevel[] = ["trace", "debug", "info", "warn", "error"];

async function onConsoleChange(event: Event): Promise<void> {
  await settingsStore.setLogLevel("console", (event.target as HTMLSelectElement).value as LogLevel);
}

async function onFileChange(event: Event): Promise<void> {
  await settingsStore.setLogLevel("file", (event.target as HTMLSelectElement).value as LogLevel);
}

const retentionDays = computed({
  get: () => settingsStore.settings.logRetentionDays,
  set: (value: number) => void settingsStore.setLogRetentionDays(value),
});
</script>

<template>
  <div class="log-level-section">
    <h2>Registro</h2>
    <p class="log-level-section__note">
      Se aplica de inmediato, sin reiniciar la app — el nivel de "Consola" controla lo que ves en vivo, "Archivo"
      controla lo que queda guardado en <code>%APPDATA%\com.warlay.lanlauncher\logs\</code>.
    </p>
    <div class="row log-level-section__row">
      <label class="field"
        >Consola
        <select
          :value="settingsStore.settings.consoleLogLevel"
          data-testid="log-level-console"
          aria-label="Nivel de registro en consola"
          @change="onConsoleChange"
        >
          <option v-for="level in LEVELS" :key="level" :value="level">{{ level }}</option>
        </select>
      </label>
      <label class="field"
        >Archivo
        <select
          :value="settingsStore.settings.fileLogLevel"
          data-testid="log-level-file"
          aria-label="Nivel de registro en archivo"
          @change="onFileChange"
        >
          <option v-for="level in LEVELS" :key="level" :value="level">{{ level }}</option>
        </select>
      </label>
    </div>

    <label class="field"
      >Días de retención de logs<BaseNumberInput
        v-model="retentionDays"
        :min="1"
        :max="365"
        test-id="log-retention-days"
        aria-label="Días de retención de logs"
    /></label>
    <p class="log-level-section__note">
      Los archivos de log con más días que este límite se borran al iniciar la app, y también al momento de cambiar este
      valor.
    </p>
  </div>
</template>

<style scoped lang="scss">
.log-level-section {
  display: flex;
  flex-direction: column;
  gap: $spacing-md;

  &__note {
    margin: 0;
    font-size: $text-sm;
    color: $text-muted;

    code {
      color: $text-dim;
      background: rgba(0, 0, 0, 0.3);
      padding: 1px 4px;
      border-radius: $radius-sm;
    }
  }

  &__row {
    gap: $spacing-lg;
  }

  select {
    background: $bg-parchment;
    border: 1px solid $border-bronze;
    color: $text;
    padding: 9px 12px;
    border-radius: $radius-md;
    font-family: $font-body;
    font-size: $text-md;
    transition: $transition;

    &:focus {
      outline: none;
      border-color: $gold;
      box-shadow: $focus-ring;
      background: rgba(255, 255, 255, 0.1);
    }
  }
}
</style>
