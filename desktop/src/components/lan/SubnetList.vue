<script setup lang="ts">
import { computed } from "vue";
import { useLanMeshStore } from "../../store/lanMesh";
import SubnetItem from "./SubnetItem.vue";

const lanMeshStore = useLanMeshStore();
const usable = computed(() => lanMeshStore.subnets.filter((subnet) => !subnet.isLoopback));
</script>

<template>
  <div class="subnet-list" data-testid="subnet-list">
    <p v-if="usable.length === 0" class="empty">No se detectaron interfaces de red activas.</p>
    <SubnetItem v-for="subnet in usable" :key="subnet.cidr" :subnet="subnet" />
  </div>
</template>

<style scoped lang="scss">
.subnet-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
</style>
