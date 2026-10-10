<script setup lang="ts">
import { onMounted, ref } from "vue";
import { ensureDaemonRunning } from "./api";
import AgentView from "./components/agent/AgentView.vue";
import BaseButton from "./components/base/BaseButton.vue";
import ConsoleBar from "./components/console/ConsoleBar.vue";
import ConsoleDrawer from "./components/console/ConsoleDrawer.vue";
import ExitBanner from "./components/layout/ExitBanner.vue";
import StatusBar from "./components/layout/StatusBar.vue";
import LanTab from "./components/lan/LanTab.vue";
import ServerRail from "./components/rail/ServerRail.vue";
import EmptyState from "./components/servers/EmptyState.vue";
import ServerRoomsView from "./components/servers/ServerRoomsView.vue";
import SettingsView from "./components/settings/SettingsView.vue";
import { bootstrap } from "./store/bootstrap";
import { useUiStore } from "./store/ui";

const uiStore = useUiStore();

// The daemon is what actually talks to the network/LAN on this machine's
// behalf; nothing else in the UI is meaningful before it's up and
// answering. Gates the whole shell so the user sees "arrancando el
// agente..." (and, if Windows needs to ask for elevation, a real error if
// they decline it) instead of a UI that renders fine but silently can't do
// anything network-related.
type DaemonGateState = "checking" | "error";
const daemonGate = ref<DaemonGateState>("checking");
const daemonError = ref("");

async function startDaemonThenApp() {
  daemonGate.value = "checking";
  try {
    await ensureDaemonRunning();
    await bootstrap();
    uiStore.daemonReady = true;
  } catch (error) {
    daemonGate.value = "error";
    daemonError.value = error instanceof Error ? error.message : String(error);
  }
}

onMounted(() => {
  void startDaemonThenApp();
});
</script>

<template>
  <div v-if="!uiStore.daemonReady" class="daemon-gate">
    <template v-if="daemonGate === 'checking'">
      <div class="daemon-gate__spinner" aria-hidden="true" />
      <p>Iniciando el agente de Warlay...</p>
    </template>
    <template v-else>
      <p class="daemon-gate__error">No se pudo iniciar el agente: {{ daemonError }}</p>
      <BaseButton @click="startDaemonThenApp">Reintentar</BaseButton>
    </template>
  </div>
  <div v-else class="shell">
    <ServerRail />
    <div class="shell__main">
      <ExitBanner />
      <main class="shell__main-content">
        <ServerRoomsView v-if="uiStore.railSelection?.kind === 'server'" :server-id="uiStore.railSelection.serverId" />
        <LanTab v-else-if="uiStore.railSelection?.kind === 'lan'" />
        <SettingsView v-else-if="uiStore.railSelection?.kind === 'settings'" />
        <AgentView v-else-if="uiStore.railSelection?.kind === 'agent'" />
        <EmptyState v-else />
      </main>
      <StatusBar />
      <ConsoleDrawer v-if="uiStore.consoleDrawerOpen" />
      <ConsoleBar />
    </div>
  </div>
</template>

<style scoped lang="scss">
.daemon-gate {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 1rem;
  height: 100vh;
  width: 100vw;
  text-align: center;
  padding: 1.5rem;
  color: $text;
  // Matches .shell's own tone -- this screen renders *instead of* .shell
  // before the daemon is ready.
  background-color: $bg-void;

  &__spinner {
    width: 2.5rem;
    height: 2.5rem;
    border-radius: 50%;
    border: 3px solid rgba(255, 255, 255, 0.2);
    border-top-color: currentColor;
    animation: daemon-gate-spin 0.8s linear infinite;
  }

  &__error {
    max-width: 32rem;
    color: $danger;
  }
}

@keyframes daemon-gate-spin {
  to {
    transform: rotate(360deg);
  }
}

.shell {
  display: flex;
  height: 100vh;
  width: 100vw;
  background-color: $bg-void;

  &__main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    height: 100%;
    position: relative;
  }

  &__main-content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 32px 40px;
    animation: fadeIn 0.5s ease;

    @media (max-width: $breakpoint-md) {
      padding: 20px 24px;
    }

    @media (max-width: $breakpoint-sm) {
      padding: 14px 16px;
    }
  }
}
</style>
