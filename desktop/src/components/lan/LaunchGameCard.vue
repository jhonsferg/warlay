<script setup lang="ts">
import { Rocket } from "@lucide/vue";
import { ref } from "vue";
import { useLanMeshStore } from "../../store/lanMesh";
import { useSettingsStore } from "../../store/settings";
import BaseButton from "../base/BaseButton.vue";
import HintText from "../base/HintText.vue";

const lanMeshStore = useLanMeshStore();
const settingsStore = useSettingsStore();
const launching = ref(false);
const hint = ref("");
const hintError = ref(false);

async function launch(): Promise<void> {
  launching.value = true;
  hintError.value = false;
  try {
    const result = await lanMeshStore.launchConfiguredGame();
    hint.value = `Proceso iniciado (PID ${result.pid}).`;
  } catch (error) {
    hint.value = `Error al lanzar el juego: ${String(error)}`;
    hintError.value = true;
  } finally {
    launching.value = false;
  }
}
</script>

<template>
  <div class="launch-game-card">
    <p class="hint">Usa el ejecutable configurado en <strong>Ajustes</strong>.</p>
    <p class="readonly-value" data-testid="lan-launch-exe-display">
      {{ settingsStore.settings.gameExecutablePath || "(sin configurar)" }}
    </p>
    <BaseButton
      variant="primary"
      :icon="Rocket"
      :icon-size="14"
      :disabled="launching"
      test-id="lan-launch-submit"
      @click="launch"
    >
      Lanzar
    </BaseButton>
    <HintText :text="hint" :error="hintError" test-id="lan-launch-hint" />
  </div>
</template>

<style scoped lang="scss">
.launch-game-card {
  display: flex;
  flex-direction: column;
  gap: $spacing-md;
  align-items: flex-start;
}
</style>
