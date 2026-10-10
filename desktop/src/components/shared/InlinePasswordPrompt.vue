<script setup lang="ts">
import { ref } from "vue";
import BaseButton from "../base/BaseButton.vue";
import BaseInput from "../base/BaseInput.vue";

withDefaults(
  defineProps<{
    placeholder?: string;
    buttonLabel?: string;
    inputTestId?: string;
    buttonTestId?: string;
    ariaLabel?: string;
  }>(),
  {
    placeholder: "Contraseña",
    buttonLabel: "Conectar",
    inputTestId: undefined,
    buttonTestId: undefined,
    ariaLabel: undefined,
  },
);
const emit = defineEmits<{ submit: [password: string] }>();

const password = ref("");

function submit(): void {
  emit("submit", password.value);
}
</script>

<template>
  <div class="input-group">
    <BaseInput
      v-model="password"
      :placeholder="placeholder"
      :test-id="inputTestId"
      :aria-label="ariaLabel ?? placeholder"
      @keydown.enter="submit"
    />
    <BaseButton variant="primary" :test-id="buttonTestId" @click="submit">{{ buttonLabel }}</BaseButton>
  </div>
</template>
