<script setup lang="ts">
import { Play, Square } from "@lucide/vue";
import { computed, ref } from "vue";
import { useLanMeshStore } from "../../store/lanMesh";
import BaseButton from "../base/BaseButton.vue";
import BaseNumberInput from "../base/BaseNumberInput.vue";
import HintText from "../base/HintText.vue";

const lanMeshStore = useLanMeshStore();
const hint = ref("");
const buttonState = ref<boolean>(false);
const hintError = ref(false);

const peersLabel = computed(() =>
  lanMeshStore.tunnelPeerAddrs.size === 0 ? "ninguno" : [...lanMeshStore.tunnelPeerAddrs].join(", "),
);

async function start(): Promise<void> {
  try {
    buttonState.value = true;
    hintError.value = false;
    hint.value = "Iniciando túnel…";
    await lanMeshStore.startTunnel();
    hint.value = `Túnel activo en 0.0.0.0:${lanMeshStore.bindPort}.`;
  } catch (error) {
    buttonState.value = false;
    hint.value = `Error al iniciar el túnel: ${String(error)}`;
    hintError.value = true;
  }
}

async function stop(): Promise<void> {
  try {
    buttonState.value = false;
    await lanMeshStore.stopTunnel();
    hint.value = "Túnel detenido.";
    hintError.value = false;
  } catch (error) {
    buttonState.value = true;
    hint.value = `Error al detener el túnel: ${String(error)}`;
    hintError.value = true;
  }
}
</script>

<template>
  <div class="tunnel-panel">
    <p class="hint">
      Este equipo también actuará como nodo del mesh, reflejando el tráfico de difusión local hacia los peers marcados
      abajo.
    </p>
    <label class="field"
      >Puerto de juego (bind)<BaseNumberInput
        v-model="lanMeshStore.bindPort"
        :min="1"
        :max="65535"
        test-id="bind-port-input"
        aria-label="Puerto de juego para el túnel"
    /></label>
    <p class="hint">
      Peers seleccionados: <span data-testid="tunnel-peers-label">{{ peersLabel }}</span>
    </p>
    <div class="row tunnel-panel__actions">
      <BaseButton
        v-if="!buttonState"
        variant="primary"
        :icon="Play"
        :icon-size="14"
        :disabled="lanMeshStore.tunnelRunning"
        test-id="start-tunnel"
        @click="start"
        >Iniciar túnel</BaseButton
      >
      <BaseButton
        v-else
        variant="danger"
        :icon="Square"
        :icon-size="14"
        :disabled="!lanMeshStore.tunnelRunning"
        test-id="stop-tunnel"
        @click="stop"
        >Detener túnel</BaseButton
      >
    </div>
    <HintText :text="hint" :error="hintError" test-id="tunnel-hint" />
  </div>
</template>

<style scoped lang="scss">
.tunnel-panel {
  display: flex;
  flex-direction: column;
  gap: $spacing-md;

  &__actions {
    gap: $spacing-sm;
  }
}
</style>
