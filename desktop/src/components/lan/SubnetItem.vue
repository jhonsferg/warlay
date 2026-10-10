<script setup lang="ts">
import { computed } from "vue";
import { useLanMeshStore } from "../../store/lanMesh";
import type { DetectedSubnet } from "../../types";
import BaseCheckbox from "../base/BaseCheckbox.vue";

const props = defineProps<{ subnet: DetectedSubnet }>();
const lanMeshStore = useLanMeshStore();

const checked = computed({
  get: () => lanMeshStore.selectedSubnetCidrs.has(props.subnet.cidr),
  set: (value: boolean) => lanMeshStore.toggleSubnetCidr(props.subnet.cidr, value),
});
</script>

<template>
  <label class="subnet-item" :data-testid="`subnet-item-${subnet.cidr}`">
    <BaseCheckbox
      v-model="checked"
      :test-id="`subnet-checkbox-${subnet.cidr}`"
      :aria-label="`Usar subred ${subnet.cidr}`"
    />
    <span class="subnet-item__name">{{ subnet.interfaceName }}</span>
    <span class="subnet-item__cidr">{{ subnet.cidr }}</span>
  </label>
</template>

<style scoped lang="scss">
.subnet-item {
  display: flex;
  align-items: center;
  gap: $spacing-sm;
  padding: 6px 8px;
  border-radius: $radius-md;
  cursor: pointer;
  transition: $transition;

  &:hover {
    background: rgba(255, 255, 255, 0.06);
  }

  &__name {
    color: $text;
    font-size: $text-md;
  }

  &__cidr {
    margin-left: auto;
    color: $text-muted;
    font-family: "Consolas", monospace;
    font-size: $text-xs;
  }
}
</style>
