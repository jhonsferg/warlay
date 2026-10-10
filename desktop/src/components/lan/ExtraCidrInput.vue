<script setup lang="ts">
import { Plus } from "@lucide/vue";
import { ref } from "vue";
import { useLanMeshStore } from "../../store/lanMesh";
import BaseButton from "../base/BaseButton.vue";
import BaseInput from "../base/BaseInput.vue";
import CidrChip from "./CidrChip.vue";

const lanMeshStore = useLanMeshStore();
const value = ref("");

function add(): void {
  lanMeshStore.addExtraCidr(value.value);
  value.value = "";
}
</script>

<template>
  <div class="input-group">
    <BaseInput
      v-model="value"
      placeholder="Subred adicional, ej. 10.0.0.0/24"
      test-id="extra-cidr-input"
      aria-label="Subred adicional en notación CIDR"
      @keydown.enter="add"
    />
    <BaseButton variant="secondary" :icon="Plus" :icon-size="14" test-id="extra-cidr-add" @click="add">
      Añadir
    </BaseButton>
  </div>
  <div class="extra-cidr-list" data-testid="extra-cidr-list">
    <CidrChip v-for="cidr in lanMeshStore.extraCidrs" :key="cidr" :cidr="cidr" />
  </div>
</template>

<style scoped lang="scss">
.extra-cidr-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: $spacing-sm;
}
</style>
