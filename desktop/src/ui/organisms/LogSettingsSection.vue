<script setup lang="ts">
import { FileText, Trash2 } from "@lucide/vue";
import { onMounted, ref } from "vue";
import { errorMessage } from "../../services/http";
import { isDesktop, native, type LogUsage } from "../../services/native";
import { useSettings } from "../../stores/settings";
import { useToasts } from "../../stores/toasts";
import WButton from "../atoms/WButton.vue";
import FormField from "../molecules/FormField.vue";
import { t, tx } from "../../services/i18n";

const settings = useSettings();
const toasts = useToasts();

/** Least to most detail; more detail means bigger files. */
const LEVELS = [
  { value: "error", label: tx("Solo errores") },
  { value: "warn", label: tx("Avisos y errores") },
  { value: "info", label: tx("Información (recomendado)") },
  { value: "debug", label: tx("Depuración (muy detallado)") },
  { value: "trace", label: tx("Traza (enorme, solo para diagnosticar)") },
];

const RETENTION_DAYS = [1, 3, 7, 14, 30, 90];

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

async function setLevel(level: string) {
  const previous = settings.fileLogLevel;
  try {
    await settings.setFileLogLevel(level);
    toasts.success(t("Nivel de detalle actualizado"), t("Se aplicó al instante."));
  } catch (e) {
    settings.fileLogLevel = previous;
    toasts.error(t("No se pudo cambiar el nivel de detalle"), errorMessage(e));
  }
}

async function setRetention(days: number) {
  try {
    await settings.setLogRetentionDays(days);
    toasts.success(t("Limpieza programada"), days === 1 ? t("Se borran los registros con más de 1 día.") : t("Se borran los registros con más de {days} días.", { days }));
    await refreshUsage();
  } catch (e) {
    toasts.error(t("No se pudo guardar la limpieza"), errorMessage(e));
  }
}

async function cleanNow(allInactive: boolean) {
  busy.value = true;
  try {
    const result = await native.cleanLogsNow(allInactive);
    if (result) {
      usage.value = result;
      toasts.success(
        result.removed > 0 ? t("{n} archivo(s) eliminado(s)", { n: result.removed }) : t("No había nada que borrar"),
        t("Quedan {n} archivo(s), {size}.", { n: result.files, size: formatBytes(result.bytes) }),
      );
    }
  } catch (e) {
    toasts.error(t("No se pudo limpiar"), errorMessage(e));
  } finally {
    busy.value = false;
  }
}

onMounted(() => {
  if (!isDesktop) return;
  void settings.load().catch(() => {});
  void refreshUsage();
});
</script>

<template>
  <div class="log-settings">
    <p v-if="!isDesktop" class="log-settings__note" data-testid="logs-desktop-only">
      {{ t("Los registros se configuran desde la aplicación de escritorio.") }}
    </p>
    <template v-else>
      <section class="log-settings__card">
        <h3><FileText /> {{ t("Nivel de detalle") }}</h3>
        <p class="log-settings__muted">
          {{ t("Más detalle significa archivos mucho más grandes. Los cambios se aplican al instante, sin reiniciar.") }}
        </p>
        <FormField :label="t('Registros de Warlay')" for="app-log-level" :hint="t('Lo que registra esta aplicación.')">
          <select
            id="app-log-level"
            class="log-settings__select"
            data-testid="app-log-level"
            :value="settings.fileLogLevel"
            @change="setLevel(($event.target as HTMLSelectElement).value)"
          >
            <option v-for="l in LEVELS" :key="l.value" :value="l.value">{{ t(l.label) }}</option>
          </select>
        </FormField>
      </section>

      <section class="log-settings__card">
        <h3><Trash2 /> {{ t("Limpieza automática") }}</h3>
        <p class="log-settings__muted">
          {{ t("Los registros más viejos se borran solos al abrir la aplicación y cada hora mientras esté abierta. El archivo que se está escribiendo nunca se borra.") }}
        </p>
        <FormField :label="t('Borrar registros con más de')" for="log-retention">
          <select
            id="log-retention"
            class="log-settings__select"
            data-testid="log-retention"
            :value="settings.logRetentionDays"
            @change="setRetention(Number(($event.target as HTMLSelectElement).value))"
          >
            <option v-for="d in RETENTION_DAYS" :key="d" :value="d">{{ d === 1 ? t("1 día") : t("{n} días", { n: d }) }}</option>
          </select>
        </FormField>
        <p class="log-settings__usage" data-testid="log-usage">
          <template v-if="usage">
            {{ t("Ahora ocupan") }} <strong>{{ formatBytes(usage.bytes) }}</strong> {{ t("en {n} archivo(s).", { n: usage.files }) }}
          </template>
          <template v-else>{{ t("No se pudo leer el tamaño de los registros.") }}</template>
        </p>
        <div class="log-settings__actions">
          <WButton variant="secondary" :loading="busy" data-testid="clean-now" @click="cleanNow(false)"
            >{{ t("Limpiar ahora") }}</WButton
          >
          <WButton variant="secondary" :loading="busy" data-testid="clean-all" @click="cleanNow(true)"
            >{{ t("Borrar todos los inactivos") }}</WButton
          >
        </div>
      </section>
    </template>
  </div>
</template>

<style scoped lang="scss">
.log-settings {
  // The two cards are stacked in one column, so the update card sits beside
  // them without leaving an empty gap under the shorter cards.
  display: flex;
  flex-direction: column;
  gap: var(--s-5);

  &__card {
    @include card(var(--s-5));
    // Cards share the column height so it lines up with the update card.
    flex: 1 1 auto;
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
