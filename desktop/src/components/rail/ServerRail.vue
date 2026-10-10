<script setup lang="ts">
import { Cpu, Network, Settings } from "@lucide/vue";
import { computed, ref } from "vue";
import { useServersStore } from "../../store/servers";
import { useUiStore } from "../../store/ui";
import AddServerModal from "../servers/AddServerModal.vue";
import AddServerRailButton from "./AddServerRailButton.vue";
import RailIconButton from "./RailIconButton.vue";
import ServerRailItem from "./ServerRailItem.vue";

const serversStore = useServersStore();
const uiStore = useUiStore();
const servers = computed(() => [...serversStore.servers.values()]);
const showAddServerModal = ref(false);

function selectLan(): void {
  uiStore.railSelection = { kind: "lan" };
}

function selectSettings(): void {
  uiStore.railSelection = { kind: "settings" };
}

function selectAgent(): void {
  uiStore.railSelection = { kind: "agent" };
}
</script>

<template>
  <nav class="server-rail" aria-label="Servidores">
    <div class="server-rail__scroll">
      <ServerRailItem v-for="server in servers" :key="server.id" :server="server" />
      <AddServerRailButton @click="showAddServerModal = true" />
    </div>
    <div class="server-rail__footer">
      <RailIconButton
        :icon="Network"
        label="LAN Local"
        :active="uiStore.railSelection?.kind === 'lan'"
        test-id="rail-lan"
        @click="selectLan"
      />
      <RailIconButton
        :icon="Settings"
        label="Ajustes"
        :active="uiStore.railSelection?.kind === 'settings'"
        test-id="rail-settings"
        @click="selectSettings"
      />
      <RailIconButton
        :icon="Cpu"
        label="Agente"
        :active="uiStore.railSelection?.kind === 'agent'"
        test-id="rail-agent"
        @click="selectAgent"
      />
    </div>
  </nav>
  <AddServerModal v-if="showAddServerModal" @close="showAddServerModal = false" />
</template>

<style scoped lang="scss">
// A macOS Finder/Mail-style source-list sidebar: vibrancy over the app's
// own backdrop (real compositor blur isn't available on Windows -- this is
// the CSS approximation), a hairline
// separator instead of a heavy drop shadow, rows inset from the edges
// rather than full-bleed.
.server-rail {
  width: 280px;
  flex-shrink: 0;
  background: $bg-stone;
  backdrop-filter: $glass-blur;
  border-right: 1px solid $border-bronze;
  display: flex;
  flex-direction: column;
  z-index: 10;
  transition: width 0.2s $ease-standard;

  // Same idea as Finder/Mail's sidebar collapsing in a narrow window --
  // the width shrinks first, and past $breakpoint-sm the server cards
  // themselves drop their secondary text (see ServerRailItem.vue) so a
  // narrower rail doesn't just clip everything mid-word.
  @media (max-width: $breakpoint-md) {
    width: 220px;
  }

  @media (max-width: $breakpoint-sm) {
    width: 180px;
  }

  &__scroll {
    flex: 1;
    overflow-y: auto;
    padding: $spacing-md;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  &__footer {
    padding: $spacing-md;
    border-top: 1px solid $border-bronze;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
}
</style>
