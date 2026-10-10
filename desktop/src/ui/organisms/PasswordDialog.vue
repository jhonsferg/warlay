<script setup lang="ts">
import { Lock } from "@lucide/vue";
import { ref } from "vue";
import WButton from "../atoms/WButton.vue";
import WInput from "../atoms/WInput.vue";
import FormField from "../molecules/FormField.vue";
import WModal from "./WModal.vue";

defineProps<{ roomName: string; error?: string | null; busy?: boolean }>();
const emit = defineEmits<{ close: []; submit: [password: string] }>();
const password = ref("");
</script>

<template>
  <WModal :title="roomName" subtitle="Esta sala está protegida con contraseña." :width="400" @close="emit('close')">
    <form id="room-password-form" @submit.prevent="emit('submit', password)">
      <FormField label="Contraseña" for="join-password" :error="error">
        <WInput id="join-password" v-model="password" type="password" :invalid="!!error">
          <template #icon><Lock /></template>
        </WInput>
      </FormField>
    </form>
    <template #footer>
      <WButton variant="ghost" @click="emit('close')">Cancelar</WButton>
      <WButton type="submit" form="room-password-form" :loading="busy" :disabled="!password">Entrar</WButton>
    </template>
  </WModal>
</template>
