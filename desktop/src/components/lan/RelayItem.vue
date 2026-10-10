<script setup lang="ts">
import { computed } from "vue";
import { useLanMeshStore } from "../../store/lanMesh";
import type { RelayInfo } from "../../types";
import { relayIp } from "../../types";
import PeerToggle from "./PeerToggle.vue";

const props = defineProps<{ relay: RelayInfo }>();
const lanMeshStore = useLanMeshStore();

const ip = computed(() => relayIp(props.relay));
const selected = computed(() => lanMeshStore.selectedRelayAddr === props.relay.addr);

function select(): void {
  void lanMeshStore.selectRelay(props.relay.addr);
}
</script>

<template>
  <div class="relay-item" :class="{ 'relay-item--selected': selected }" :data-testid="`relay-item-${relay.addr}`">
    <div
      class="relay-item__main"
      role="button"
      tabindex="0"
      :aria-label="`Seleccionar relay ${relay.serverName || relay.addr}`"
      :data-testid="`relay-select-${relay.addr}`"
      @click="select"
      @keydown.enter="select"
    >
      <div class="relay-item__name">{{ relay.serverName || "(sin nombre)" }}</div>
      <div class="relay-item__meta">
        {{ ip }} · relay:{{ relay.relayPort }} · control:{{ lanMeshStore.controlPort }} · {{ relay.knownPeers }} peers ·
        {{ relay.uptimeSecs }}s activo
      </div>
    </div>
    <PeerToggle :addr="relay.addr" />
  </div>
</template>

<style scoped lang="scss">
.relay-item {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: $spacing-md;
  padding: $spacing-sm $spacing-md;
  background: rgba(255, 255, 255, 0.04);
  border: none;
  border-radius: $radius-md;
  transition: $transition;

  // Same accent-tinted selection as the rest of the app, replacing the
  // old steel "you're here" convention.
  &--selected {
    background: $gold-dim;
  }

  &__main {
    flex: 1;
    min-width: 0;
    cursor: pointer;
    border-radius: $radius-sm;

    &:focus-visible {
      outline: none;
      box-shadow: $focus-ring;
    }
  }

  &__name {
    color: $text;
    font-size: $text-md;
    font-weight: 600;
  }

  &__meta {
    font-size: $text-xs;
    color: $text-muted;
  }
}
</style>
