<script setup lang="ts">
import { toTypedSchema } from "@vee-validate/zod";
import { ref } from "vue";
import { useField } from "vee-validate";
import { useServersStore } from "../../store/servers";
import { serverAddressSchema } from "../../utils/validation";
import BaseButton from "../base/BaseButton.vue";
import BaseInput from "../base/BaseInput.vue";
import HintText from "../base/HintText.vue";

const emit = defineEmits<{ added: [] }>();
const serversStore = useServersStore();

const {
  value: address,
  errorMessage,
  validate,
  resetField,
} = useField<string>("address", toTypedSchema(serverAddressSchema));
const submitError = ref("");

async function submit(): Promise<void> {
  submitError.value = "";
  const result = await validate();
  if (!result.valid || !result.value) return;
  try {
    await serversStore.addServer(result.value.trim());
    resetField({ value: "" });
    emit("added");
  } catch (error) {
    submitError.value = `Error al registrar: ${String(error)}`;
  }
}
</script>

<template>
  <div class="add-server-form">
    <div class="form-controls">
      <label class="form-controls__label field">Dirección (wss://host o host:puerto)</label>
      <BaseInput
        v-model="address"
        placeholder="wss://warlay.codesoftlabs.com"
        test-id="add-server-address-input"
        aria-label="Dirección del servidor"
        @keydown.enter="submit"
      />
    </div>
    <p class="add-server-form__hint">
      El nombre del servidor lo aporta él mismo al conectar - no hace falta escribirlo.
    </p>
    <BaseButton variant="primary" test-id="add-server-submit" @click="submit" :full="true"
      >Registrar servidor</BaseButton
    >
    <HintText :text="errorMessage || submitError" :error="!!(errorMessage || submitError)" test-id="add-server-hint" />
  </div>
</template>

<style scoped lang="scss">
.add-server-form {
  display: flex;
  flex-direction: column;
  gap: $spacing-md;
  align-items: flex-start;

  &__hint {
    margin: 0;
    font-size: $text-sm;
  }

  .form-controls {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;

    &__label {
      font-size: $text-sm;
      flex-shrink: 0;
    }
  }
}
</style>
