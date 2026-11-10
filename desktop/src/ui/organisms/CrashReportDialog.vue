<script setup lang="ts">
import { Bug } from "@lucide/vue";
import { computed, ref } from "vue";
import { errorMessage } from "../../services/http";
import { t } from "../../services/i18n";
import { useCrashReports } from "../../stores/crashReports";
import { useToasts } from "../../stores/toasts";
import WButton from "../atoms/WButton.vue";
import WModal from "./WModal.vue";

/** Offers the crash report an earlier run left, and lets the player decide what to do with it. */
const crashes = useCrashReports();
const toasts = useToasts();

const showing = ref(false);
const busy = ref(false);
const more = computed(() => crashes.reports.length - 1);

async function toggleDetails() {
  showing.value = !showing.value;
  if (showing.value && crashes.detail === null) await crashes.loadDetail();
}

async function copy() {
  if (crashes.detail === null) await crashes.loadDetail();
  try {
    await navigator.clipboard.writeText(crashes.detail ?? "");
    toasts.success(t("Informe copiado"));
  } catch {
    toasts.error(t("No se pudo copiar"));
  }
}

async function send() {
  busy.value = true;
  try {
    await crashes.sendToGitHub();
    toasts.success(t("Se abrió GitHub"), t("Revisa el texto y envíalo si quieres."));
  } catch (e) {
    toasts.error(t("No se pudo abrir el navegador"), errorMessage(e));
  } finally {
    busy.value = false;
  }
}

async function discard() {
  showing.value = false;
  await crashes.discard();
}

function later() {
  showing.value = false;
  crashes.postpone();
}
</script>

<template>
  <WModal v-if="crashes.current" :title="t('Warlay se cerró de forma inesperada')" :width="560" @close="later">
    <div class="crash" data-testid="crash-dialog">
      <p class="crash__lead">
        <Bug :size="22" />
        <span>{{
          t(
            "La última vez algo falló. El informe se guardó solo en este equipo: no se envió a nadie. Si quieres, ayúdanos a arreglarlo enviándolo.",
          )
        }}</span>
      </p>
      <p class="crash__what" data-testid="crash-title">{{ crashes.current.title }}</p>
      <p v-if="more > 0" class="crash__more">{{ t("Hay {n} informes más.", { n: more }) }}</p>
      <pre v-if="showing" class="crash__detail" data-testid="crash-detail">{{ crashes.detail ?? "…" }}</pre>
      <p class="crash__note">
        {{
          t(
            "Se abre una incidencia en GitHub con el informe ya escrito, para que la revises antes de enviarla. El informe no incluye tu nombre de usuario de Windows, tus contraseñas ni tus mensajes.",
          )
        }}
      </p>
    </div>
    <template #footer>
      <WButton variant="ghost" @click="later">{{ t("Ahora no") }}</WButton>
      <WButton variant="ghost" data-testid="crash-discard" @click="discard">{{ t("Descartar") }}</WButton>
      <WButton variant="secondary" data-testid="crash-details" @click="toggleDetails">{{
        showing ? t("Ocultar detalles") : t("Ver detalles")
      }}</WButton>
      <WButton variant="secondary" data-testid="crash-copy" @click="copy">{{ t("Copiar") }}</WButton>
      <WButton :loading="busy" data-testid="crash-send" @click="send">{{ t("Reportar en GitHub") }}</WButton>
    </template>
  </WModal>
</template>

<style scoped lang="scss">
.crash {
  display: flex;
  flex-direction: column;
  gap: var(--s-3);

  &__lead {
    display: flex;
    gap: var(--s-3);
    align-items: flex-start;
    margin: 0;
    color: var(--text-1);

    svg {
      flex: none;
      color: var(--accent);
      margin-top: 2px;
    }
  }

  &__what {
    margin: 0;
    padding: var(--s-3);
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-sm);
    color: var(--text-1);
    background: var(--bg-1);
    border: 1px solid var(--line-2);
    border-radius: var(--r-sm);
    overflow-wrap: anywhere;
  }

  &__more,
  &__note {
    margin: 0;
    color: var(--text-3);
    font-size: var(--fs-sm);
  }

  &__detail {
    max-height: 240px;
    margin: 0;
    padding: var(--s-3);
    overflow: auto;
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
    color: var(--text-2);
    background: var(--bg-1);
    border: 1px solid var(--line-2);
    border-radius: var(--r-sm);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
}
</style>
