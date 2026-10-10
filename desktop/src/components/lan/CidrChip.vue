<script setup lang="ts">
import { X } from "@lucide/vue";
import { useLanMeshStore } from "../../store/lanMesh";

const props = defineProps<{ cidr: string }>();
const lanMeshStore = useLanMeshStore();

function remove(): void {
  lanMeshStore.removeExtraCidr(props.cidr);
}
</script>

<template>
  <span class="chip" :data-testid="`extra-cidr-chip-${cidr}`"
    >{{ cidr
    }}<button
      class="chip__remove"
      type="button"
      :aria-label="`Quitar subred ${cidr}`"
      :data-testid="`extra-cidr-remove-${cidr}`"
      @click="remove"
    >
      <X :size="12" /></button
  ></span>
</template>

<style scoped lang="scss">
.chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px;
  background: $gold-dim;
  border: none;
  border-radius: $radius-full;
  color: $gold-bright;
  font-family: "Consolas", monospace;
  font-size: $text-xs;
}

.chip__remove {
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: $gold-bright;
  cursor: pointer;
  padding: 0;
  opacity: 0.7;

  &:hover {
    opacity: 1;
  }
}
</style>
