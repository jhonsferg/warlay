<script setup lang="ts">
import { computed } from "vue";
import { useLanMeshStore } from "../../store/lanMesh";
import RelayItem from "./RelayItem.vue";

const lanMeshStore = useLanMeshStore();
const relays = computed(() => [...lanMeshStore.relays.values()]);
</script>

<template>
  <div class="relay-list" data-testid="relay-list">
    <p v-if="relays.length === 0" class="empty">Ningún servidor descubierto todavía.</p>
    <RelayItem v-for="relay in relays" :key="relay.addr" :relay="relay" />
  </div>
</template>

<style scoped lang="scss">
.relay-list {
  display: flex;
  flex-direction: column;
  gap: $spacing-sm;
}
</style>
