<script setup lang="ts">
import { FileText, Trash2 } from "@lucide/vue";
import { onMounted, ref } from "vue";
import { errorMessage } from "../../services/http";
import { isDesktop, native, type AgentSettings, type LogUsage } from "../../services/native";
import { useSettings } from "../../stores/settings";
import { useToasts } from "../../stores/toasts";
import WButton from "../atoms/WButton.vue";
import FormField from "../molecules/FormField.vue";

const settings = useSettings();
const toasts = useToasts();

/** Least to most detail; more detail means bigger files. */
const LEVELS = [
  { value: "error", label: "Solo errores" },
  { value: "warn", label: "Avisos y errores" },
  { value: "info", label: "Información (recomendado)" },
  { value: "debug", label: "Depuración (muy detallado)" },
  { value: "trace", label: "Traza (enorme, solo para diagnosticar)" },
];

const RETENTION_DAYS = [1, 3, 7, 14, 30, 90];

const agent = ref<AgentSettings | null>(null);
const agentUnavailable = ref(false);
const usage = ref<LogUsage | null>(null);
const busy = ref(false);

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

async function refreshUsage() {
  try {
    usage.value = (await native.logUsage()) ?? null;
  } catch {
    usage.value = null;
  }
}

async function setAgentLevel(level: string) {
  if (!agent.value) return;
  const previous = agent.value;
  agent.value = { ...previous, logLevel: level };
  try {
    await native.saveAgentSettings(agent.value);
    toasts.success("Nivel del agente actualizado", "Se aplicó al instante.");
  } catch (e) {
    agent.value = previous;
    toasts.error("No se pudo cambiar el nivel del agente", errorMessage(e));
  }
}

async function setAppLevel(level: string) {
  const previous = settings.fileLogLevel;
  try {
    await settings.setFileLogLevel(level);
    toasts.success("Nivel de la aplicación actualizado", "Se aplicó al instante.");
  } catch (e) {
    settings.fileLogLevel = previous;
    toasts.error("No se pudo cambiar el nivel de la aplicación", errorMessage(e));
  }
}

async function setRetention(days: number) {
  try {
    await settings.setLogRetentionDays(days);
    toasts.success("Limpieza programada", `Se borran los registros con más de ${days} ${days === 1 ? "día" : "días"}.`);
    await refreshUsage();
  } catch (e) {
    toasts.error("No se pudo guardar la limpieza", errorMessage(e));
  }
}

async function cleanNow(allInactive: boolean) {
  busy.value = true;
  try {
    const result = await native.cleanLogsNow(allInactive);
    if (result) {
      usage.value = result;
      toasts.success(
        result.removed > 0 ? `${result.removed} archivo(s) eliminado(s)` : "No había nada que borrar",
        `Quedan ${result.files} archivo(s), ${formatBytes(result.bytes)}.`,
      );
    }
  } catch (e) {
    toasts.error("No se pudo limpiar", errorMessage(e));
  } finally {
    busy.value = false;
  }
}

onMounted(async () => {
  if (!isDesktop) return;
  void settings.load().catch(() => {});
  void refreshUsage();
  try {
    agent.value = (await native.agentSettings()) ?? null;
  } catch {
    agent.value = null;
  }
  agentUnavailable.value = agent.value === null;
});
</script>

<template>
  <div class="log-settings">
    <p v-if="!isDesktop" class="log-settings__note" data-testid="logs-desktop-only">
      Los registros se configuran desde la aplicación de escritorio.
    </p>
    <template v-else>
      <section class="log-settings__card">
        <h3><FileText /> Nivel de detalle</h3>
        <p class="log-settings__muted">
          Más detalle significa archivos mucho más grandes. Los cambios se aplican al instante, sin reiniciar.
        </p>
        <FormField
          label="Agente de red (wnetd)"
          for="agent-log-level"
          hint="El que más registra: es quien ve el tráfico del juego."
        >
          <select
            id="agent-log-level"
            class="log-settings__select"
            data-testid="agent-log-level"
            :disabled="!agent"
            :value="agent?.logLevel ?? 'info'"
            @change="setAgentLevel(($event.target as HTMLSelectElement).value)"
          >
            <option v-for="l in LEVELS" :key="l.value" :value="l.value">{{ l.label }}</option>
          </select>
        </FormField>
        <p v-if="agentUnavailable" class="log-settings__muted">
          El agente no responde ahora; el nivel se podrá cambiar cuando esté activo.
        </p>
        <FormField label="Aplicación" for="app-log-level" hint="Lo que registra esta ventana.">
          <select
            id="app-log-level"
            class="log-settings__select"
            data-testid="app-log-level"
            :value="settings.fileLogLevel"
            @change="setAppLevel(($event.target as HTMLSelectElement).value)"
          >
            <option v-for="l in LEVELS" :key="l.value" :value="l.value">{{ l.label }}</option>
          </select>
        </FormField>
      </section>

      <section class="log-settings__card">
        <h3><Trash2 /> Limpieza automática</h3>
        <p class="log-settings__muted">
          Los registros más viejos se borran solos al abrir la aplicación y cada hora mientras esté abierta. El archivo
          que se está escribiendo nunca se borra.
        </p>
        <FormField label="Borrar registros con más de" for="log-retention">
          <select
            id="log-retention"
            class="log-settings__select"
            data-testid="log-retention"
            :value="settings.logRetentionDays"
            @change="setRetention(Number(($event.target as HTMLSelectElement).value))"
          >
            <option v-for="d in RETENTION_DAYS" :key="d" :value="d">{{ d }} {{ d === 1 ? "día" : "días" }}</option>
          </select>
        </FormField>
        <p class="log-settings__usage" data-testid="log-usage">
          <template v-if="usage">
            Ahora ocupan <strong>{{ formatBytes(usage.bytes) }}</strong> en {{ usage.files }} archivo(s).
          </template>
          <template v-else>No se pudo leer el tamaño de los registros.</template>
        </p>
        <div class="log-settings__actions">
          <WButton variant="secondary" :loading="busy" data-testid="clean-now" @click="cleanNow(false)"
            >Limpiar ahora</WButton
          >
          <WButton variant="secondary" :loading="busy" data-testid="clean-all" @click="cleanNow(true)"
            >Borrar todos los inactivos</WButton
          >
        </div>
      </section>
    </template>
  </div>
</template>

<style scoped lang="scss">
.log-settings {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
  gap: var(--s-5);

  &__card {
    @include card(var(--s-5));
    display: flex;
    flex-direction: column;
    gap: var(--s-4);

    h3 {
      display: flex;
      align-items: center;
      gap: var(--s-2);
      margin: 0;
    }
  }
  &__muted,
  &__note {
    margin: 0;
    color: var(--text-3);
    font-size: var(--fs-sm);
  }
  &__usage {
    margin: 0;
    color: var(--text-2);
  }
  &__select {
    width: 100%;
    padding: 10px var(--s-3);
    color: var(--text-1);
    background: var(--bg-4);
    border: 1px solid var(--line-2);
    border-radius: var(--r-sm);
    &:focus-visible {
      outline: 2px solid var(--accent);
    }
    &:disabled {
      opacity: 0.55;
    }
  }
  &__actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s-2);
  }
}
</style>
