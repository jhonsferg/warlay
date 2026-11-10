<script setup lang="ts">
import { Mic, ShieldAlert } from "@lucide/vue";
import { native } from "../../services/native";
import WButton from "../atoms/WButton.vue";
import WModal from "./WModal.vue";
import { t } from "../../services/i18n";

/**
 * Warlay's own microphone request, shown the first time instead of the
 * browser engine's default prompt; and, if Windows itself blocks desktop apps
 * from the microphone, the guide to switch it on.
 */
defineProps<{ mode: "ask" | "blocked" }>();
const emit = defineEmits<{ allow: []; deny: []; close: [] }>();
</script>

<template>
  <WModal
    v-if="mode === 'ask'"
    :title="t('Usar tu micrófono')"
    :subtitle="t('Para hablar con los jugadores de la sala.')"
    :width="440"
    @close="emit('deny')"
  >
    <div class="mic-dialog" data-testid="mic-consent-dialog">
      <Mic class="mic-dialog__icon" :size="28" />
      <ul>
        <li>{{ t("Warlay solo escucha mientras estés unido a la voz y no lo hayas silenciado.") }}</li>
        <li>{{ t("El audio va directo a los jugadores de tu sala, sin pasar por ningún servidor.") }}</li>
        <li>{{ t("Puedes silenciarte o salir de la voz cuando quieras, también con el atajo global.") }}</li>
      </ul>
    </div>
    <template #footer>
      <WButton variant="ghost" data-testid="mic-consent-deny" @click="emit('deny')">{{ t("Ahora no") }}</WButton>
      <WButton data-testid="mic-consent-allow" @click="emit('allow')">
        <template #icon><Mic /></template>{{ t("Permitir micrófono") }}
      </WButton>
    </template>
  </WModal>

  <WModal
    v-else
    :title="t('Windows bloquea el micrófono')"
    :subtitle="t('Ya diste permiso a Warlay, pero el sistema no lo deja pasar.')"
    :width="440"
    @close="emit('close')"
  >
    <div class="mic-dialog" data-testid="mic-blocked-dialog">
      <ShieldAlert class="mic-dialog__icon mic-dialog__icon--warn" :size="28" />
      <ul>
        <li>Abre Configuración > Privacidad y seguridad > {{ t("Micrófono.") }}</li>
        <li>{{ t("Activa \"Acceso al micrófono\" y \"Permitir que las aplicaciones de escritorio accedan a tu micrófono\".") }}</li>
        <li>{{ t("Vuelve aquí y pulsa de nuevo el botón del micrófono.") }}</li>
      </ul>
    </div>
    <template #footer>
      <WButton variant="ghost" @click="emit('close')">{{ t("Cerrar") }}</WButton>
      <WButton data-testid="mic-open-settings" @click="native.openMicPrivacySettings()">
        {{ t("Abrir configuración de Windows") }}
      </WButton>
    </template>
  </WModal>
</template>

<style scoped lang="scss">
.mic-dialog {
  display: flex;
  gap: var(--s-4, 16px);
  align-items: flex-start;

  ul {
    margin: 0;
    padding-left: var(--s-4, 16px);
    display: grid;
    gap: var(--s-2, 8px);
    font-size: 0.9rem;
  }

  &__icon {
    flex: none;
    color: var(--accent, #5b8def);

    &--warn {
      color: var(--warning, #e0a43a);
    }
  }
}
</style>
