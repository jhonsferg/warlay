<script setup lang="ts">
import { RefreshCw } from "@lucide/vue";
import { computed, onMounted, onUnmounted } from "vue";
import { useAgentStore } from "../../store/agent";
import type { LogLevel, PeerLinkStatus } from "../../types";
import BaseBadge from "../base/BaseBadge.vue";
import BaseButton from "../base/BaseButton.vue";
import BaseCheckbox from "../base/BaseCheckbox.vue";

const agentStore = useAgentStore();
const LEVELS: LogLevel[] = ["trace", "debug", "info", "warn", "error"];

async function onLogLevelChange(event: Event): Promise<void> {
  await agentStore.saveSettings({
    ...agentStore.settings,
    logLevel: (event.target as HTMLSelectElement).value as LogLevel,
  });
}

const lanDiscoveryEnabled = computed({
  get: () => agentStore.settings.lanDiscoveryEnabled,
  set: (value: boolean) => void agentStore.saveSettings({ ...agentStore.settings, lanDiscoveryEnabled: value }),
});

function peerTone(state: PeerLinkStatus["state"]): "success" | "danger" | "amber" | "steel" | "neutral" {
  switch (state) {
    case "connected":
      return "success";
    case "connecting":
      return "steel";
    case "failed":
      return "danger";
    default:
      return "neutral";
  }
}

function peerLabel(state: PeerLinkStatus["state"]): string {
  switch (state) {
    case "connected":
      return "Conectado (P2P directo)";
    case "connecting":
      return "Conectando…";
    case "failed":
      return "Falló";
    default:
      return "Sin intentar";
  }
}

function formatUptime(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return m > 0 ? `${m}m ${s}s` : `${s}s`;
}

// Same 3s cadence as the daemon's own P2P mesh retry loop
// (`crates/engine/src/p2p_mesh.rs`'s `attempt_interval`) -- a faster poll
// here wouldn't show anything new between ticks.
let statusInterval: ReturnType<typeof setInterval> | undefined;

onMounted(async () => {
  await agentStore.loadSettings();
  await agentStore.refreshStatus();
  statusInterval = setInterval(() => void agentStore.refreshStatus(), 3000);
});

onUnmounted(() => {
  if (statusInterval) clearInterval(statusInterval);
});

const hasFailedPeers = computed(() => agentStore.status?.meshPeers.some((p) => p.state === "failed") ?? false);
</script>

<template>
  <section class="agent-view" data-testid="agent-view">
    <div class="card agent-view__card">
      <h2>Agente</h2>
      <p class="agent-view__note">
        Configuración del proceso en segundo plano (<code>wnetd.exe</code>) que maneja la red, el túnel y la malla P2P —
        separado del cliente, con su propia configuración. Los cambios se aplican de inmediato, sin reiniciar.
      </p>

      <label class="field"
        >Nivel de log del agente
        <select
          :value="agentStore.settings.logLevel"
          data-testid="agent-log-level"
          aria-label="Nivel de registro del agente"
          @change="onLogLevelChange"
        >
          <option v-for="level in LEVELS" :key="level" :value="level">{{ level }}</option>
        </select>
      </label>
      <p class="agent-view__note">Se escribe en <code>%APPDATA%\com.warlay.lanlauncher\logs\wnetd-*.log</code>.</p>

      <label class="row agent-view__toggle-row">
        <BaseCheckbox
          v-model="lanDiscoveryEnabled"
          test-id="agent-lan-discovery-toggle"
          aria-label="Descubrimiento LAN (W3GS)"
        />
        Descubrimiento LAN (W3GS)
      </label>
      <p class="agent-view__note">
        Reenvía los broadcasts de "Red de Área Local" de Warcraft III a través del túnel para que aparezcas ahí del otro
        lado — es también la mayor fuente de líneas en el log del agente. Desactívalo si no lo necesitas.
      </p>
    </div>

    <div class="card agent-view__card">
      <h2>Estado del agente</h2>
      <dl class="agent-view__status-grid">
        <dt>Actividad</dt>
        <dd>{{ agentStore.status ? formatUptime(agentStore.status.uptimeSecs) : "—" }}</dd>
        <dt>Hubs conectados</dt>
        <dd>{{ agentStore.status?.hubConnections ?? "—" }}</dd>
        <dt>Túnel activo</dt>
        <dd>{{ agentStore.status ? (agentStore.status.tunnelActive ? "Sí" : "No") : "—" }}</dd>
      </dl>

      <template v-if="agentStore.status && agentStore.status.meshPeers.length > 0">
        <h3>Malla P2P</h3>
        <ul class="agent-view__peer-list">
          <li v-for="peer in agentStore.status.meshPeers" :key="peer.clientId" class="agent-view__peer">
            <span class="agent-view__peer-name"
              >{{ peer.isHost ? "Anfitrión" : "Jugador" }} · {{ peer.clientId.slice(0, 8) }}</span
            >
            <BaseBadge :tone="peerTone(peer.state)">{{ peerLabel(peer.state) }}</BaseBadge>
          </li>
        </ul>
        <BaseButton
          v-if="hasFailedPeers"
          variant="secondary"
          :icon="RefreshCw"
          :disabled="agentStore.loading"
          test-id="agent-retry-p2p"
          @click="agentStore.retryPeerMesh"
        >
          Reintentar conexión P2P
        </BaseButton>
      </template>
      <p v-else class="agent-view__note">No hay una sala activa con otros jugadores ahora mismo.</p>
    </div>
  </section>
</template>

<style scoped lang="scss">
.agent-view {
  column-width: 460px;
  column-gap: $spacing-lg;

  &__card {
    max-width: 560px;
    display: flex;
    flex-direction: column;
    gap: $spacing-md;
    break-inside: avoid;
    margin: 0 0 $spacing-lg;

    h3 {
      margin: $spacing-sm 0 0;
      font-size: $text-md;
      border-bottom: 1px solid $border-bronze;
      padding-bottom: 0.4rem;
    }
  }

  &__note {
    margin: 0;
    font-size: $text-sm;
    color: $text-muted;

    code {
      color: $text-dim;
      background: rgba(0, 0, 0, 0.3);
      padding: 1px 4px;
      border-radius: $radius-sm;
    }
  }

  &__status-grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px $spacing-md;
    margin: 0;

    dt {
      color: $text-muted;
      font-size: $text-sm;
    }

    dd {
      margin: 0;
      font-weight: 600;
    }
  }

  &__peer-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: $spacing-sm;
  }

  &__peer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: $spacing-md;
  }

  &__peer-name {
    font-size: $text-sm;
    color: $text-dim;
  }

  &__toggle-row {
    align-items: center;
    gap: $spacing-sm;
  }

  select {
    background: $bg-parchment;
    border: 1px solid $border-bronze;
    color: $text;
    padding: 9px 12px;
    border-radius: $radius-md;
    font-family: $font-body;
    font-size: $text-md;
    transition: $transition;

    &:focus {
      outline: none;
      border-color: $gold;
      box-shadow: $focus-ring;
      background: rgba(255, 255, 255, 0.1);
    }
  }
}
</style>
