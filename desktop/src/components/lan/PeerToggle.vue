<script setup lang="ts">
import { computed } from "vue";
import { useLanMeshStore } from "../../store/lanMesh";
import BaseCheckbox from "../base/BaseCheckbox.vue";

const props = defineProps<{ addr: string }>();
const lanMeshStore = useLanMeshStore();

const checked = computed({
  get: () => lanMeshStore.tunnelPeerAddrs.has(props.addr),
  set: (value: boolean) => lanMeshStore.toggleTunnelPeer(props.addr, value),
});
</script>

<template>
  <label class="peer-toggle">
    <BaseCheckbox v-model="checked" :test-id="`peer-toggle-${addr}`" :aria-label="`Usar ${addr} como peer del túnel`" />
    usar en túnel
  </label>
</template>

<style scoped lang="scss">
.peer-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  font-size: $text-xs;
  color: $text-dim;
  cursor: pointer;
  white-space: nowrap;
}
</style>
