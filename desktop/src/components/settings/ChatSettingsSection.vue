<script setup lang="ts">
import { computed } from "vue";
import { chatMessageLimitSchema } from "../../utils/validation";
import BaseNumberInput from "../base/BaseNumberInput.vue";
import HintText from "../base/HintText.vue";

const limit = defineModel<number>({ default: 500 });

// Informational only — `SettingsView`'s autosave keeps writing through
// regardless (the number input's own min/max already keeps the value in a
// sane range), this just surfaces a real message if it somehow isn't.
const validation = computed(() => chatMessageLimitSchema.safeParse(limit.value));
const errorMessage = computed(() =>
  validation.value.success ? "" : (validation.value.error.issues[0]?.message ?? ""),
);
</script>

<template>
  <div class="chat-settings-section">
    <h2>Chat</h2>
    <p class="chat-settings-section__note">
      El chat no se guarda en el servidor ni entre reinicios — cada sala de chat conserva en tu equipo solo los últimos
      mensajes que quepan en este límite; los más antiguos se descartan automáticamente.
    </p>
    <label class="field"
      >Mensajes guardados por chat<BaseNumberInput
        v-model="limit"
        :min="20"
        :max="2000"
        :step="20"
        test-id="settings-chat-limit-input"
        aria-label="Mensajes guardados por chat"
    /></label>
    <HintText :text="errorMessage" :error="!!errorMessage" test-id="settings-chat-limit-hint" />
  </div>
</template>

<style scoped lang="scss">
.chat-settings-section {
  display: flex;
  flex-direction: column;
  gap: $spacing-md;

  &__note {
    margin: 0;
    font-size: $text-sm;
    color: $text-muted;
  }
}
</style>
