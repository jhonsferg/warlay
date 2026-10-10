<script setup lang="ts">
import { Search } from "@lucide/vue";
import { ref } from "vue";
import { useLanMeshStore } from "../../store/lanMesh";
import BaseButton from "../base/BaseButton.vue";
import BaseNumberInput from "../base/BaseNumberInput.vue";
import HintText from "../base/HintText.vue";

const lanMeshStore = useLanMeshStore();
const scanning = ref(false);
const hint = ref("");
const hintError = ref(false);

async function scan(): Promise<void> {
  scanning.value = true;
  hintError.value = false;
  hint.value = "Escaneando…";
  try {
    const total = await lanMeshStore.runScan();
    hint.value = `Escaneo completo: ${total} servidor(es) encontrado(s).`;
  } catch (error) {
    hint.value = `Error durante el escaneo: ${String(error)}`;
    hintError.value = true;
  } finally {
    scanning.value = false;
  }
}
</script>

<template>
  <div class="scan-controls">
    <div class="row scan-controls__ports">
      <label class="field"
        >Puerto de control<BaseNumberInput
          v-model="lanMeshStore.controlPort"
          :min="1"
          :max="65535"
          test-id="control-port-input"
          aria-label="Puerto de control"
      /></label>
      <label class="field"
        >Timeout (ms)<BaseNumberInput
          v-model="lanMeshStore.discoveryTimeoutMs"
          :min="200"
          :max="15000"
          :step="100"
          test-id="discovery-timeout-input"
          aria-label="Timeout de descubrimiento en milisegundos"
      /></label>
    </div>
    <BaseButton
      variant="primary"
      :icon="Search"
      :icon-size="14"
      :disabled="scanning"
      test-id="scan-submit"
      @click="scan"
    >
      Buscar servidores
    </BaseButton>
    <HintText :text="hint" :error="hintError" test-id="scan-hint" />
  </div>
</template>

<style scoped lang="scss">
.scan-controls {
  display: flex;
  flex-direction: column;
  gap: $spacing-md;
  align-items: flex-start;

  &__ports {
    gap: $spacing-md;
    flex-wrap: wrap;
  }
}
</style>
