<script setup lang="ts">
import { ref } from "vue";
import { reports } from "../../services/api";
import { errorMessage } from "../../services/http";
import { canSendReport, REPORT_DETAILS_MAX, REPORT_REASONS } from "../../services/reports";
import { useDialogs } from "../../stores/dialogs";
import { useToasts } from "../../stores/toasts";
import WButton from "../atoms/WButton.vue";
import FormField from "../molecules/FormField.vue";
import WModal from "./WModal.vue";
import { t } from "../../services/i18n";

/** Reports the player in `dialogs.report`; opened with `dialogs.openReport(user, messageId?)`. */
const dialogs = useDialogs();
const toasts = useToasts();

const reason = ref("");
const details = ref("");
const sending = ref(false);

function close() {
  dialogs.report = null;
  reason.value = "";
  details.value = "";
}

async function send() {
  const target = dialogs.report;
  if (!target || !canSendReport(reason.value, details.value)) return;
  sending.value = true;
  try {
    await reports.send({
      userId: target.user.id,
      reason: reason.value,
      details: details.value.trim() || undefined,
      messageId: target.messageId,
    });
    toasts.success(t("Reporte enviado"), t("Gracias: lo revisará quien administra el servidor."));
    close();
  } catch (e) {
    toasts.error(t("No se pudo enviar el reporte"), errorMessage(e));
  } finally {
    sending.value = false;
  }
}
</script>

<template>
  <WModal
    v-if="dialogs.report"
    :title="t('Reportar a {name}', { name: dialogs.report.user.displayName })"
    :subtitle="`@${dialogs.report.user.username}`"
    @close="close"
  >
    <form class="report" data-testid="report-dialog" @submit.prevent="send">
      <FormField :label="t('Motivo')" for="report-reason">
        <select id="report-reason" v-model="reason" class="report__select" data-testid="report-reason">
          <option value="" disabled>{{ t("Elige un motivo") }}</option>
          <option v-for="r in REPORT_REASONS" :key="r.id" :value="r.id">{{ t(r.label) }}</option>
        </select>
      </FormField>
      <FormField
        :label="t('Detalles (opcional)')"
        for="report-details"
        :hint="`${details.length} / ${REPORT_DETAILS_MAX}`"
      >
        <textarea
          id="report-details"
          v-model="details"
          class="report__text"
          rows="4"
          :maxlength="REPORT_DETAILS_MAX"
          :placeholder="t('Qué pasó, y dónde (sala, comunidad o chat).')"
        />
      </FormField>
      <p class="report__note">
        {{
          t(
            "El reporte llega a quien administra el servidor. No se avisa a la otra persona. Para dejar de ver sus mensajes, bloquéala.",
          )
        }}
      </p>
    </form>
    <template #footer>
      <WButton variant="ghost" @click="close">{{ t("Cancelar") }}</WButton>
      <WButton :disabled="!canSendReport(reason, details)" :loading="sending" data-testid="report-send" @click="send">
        {{ t("Enviar reporte") }}
      </WButton>
    </template>
  </WModal>
</template>

<style scoped lang="scss">
.report {
  display: flex;
  flex-direction: column;
  gap: var(--s-4);

  &__select,
  &__text {
    width: 100%;
    padding: var(--s-3);
    font: inherit;
    color: var(--text-1);
    background: var(--bg-1);
    border: 1px solid var(--line-2);
    border-radius: var(--r-sm);
  }
  &__text {
    resize: vertical;
  }
  &__note {
    margin: 0;
    color: var(--text-3);
    font-size: var(--fs-sm);
  }
}
</style>
