<script setup lang="ts">
import { AtSign, KeyRound, Mail, User } from "@lucide/vue";
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { ApiError, errorMessage } from "../services/http";
import { useSession } from "../stores/session";
import WButton from "../ui/atoms/WButton.vue";
import WInput from "../ui/atoms/WInput.vue";
import FormField from "../ui/molecules/FormField.vue";
import AuthLayout from "../ui/templates/AuthLayout.vue";

const session = useSession();
const router = useRouter();
const username = ref("");
const displayName = ref("");
const email = ref("");
const password = ref("");
const busy = ref(false);
const error = ref<string | null>(null);
const fieldError = ref<Record<string, string>>({});
const touched = ref(false);

// Validate as the user types, mirroring the server's own rules.
const errors = computed(() => {
  const e: Record<string, string> = {};
  if (username.value && !/^[A-Za-z0-9_]{3,20}$/.test(username.value))
    e.username = "3 a 20 caracteres: letras, números o _";
  if (email.value && !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email.value)) e.email = "Correo no válido";
  if (password.value && password.value.length < 8) e.password = "Al menos 8 caracteres";
  return { ...e, ...fieldError.value };
});
const strength = computed(() => {
  const p = password.value;
  let score = 0;
  if (p.length >= 8) score++;
  if (p.length >= 12) score++;
  if (/[A-Z]/.test(p) && /[a-z]/.test(p)) score++;
  if (/\d/.test(p) && /[^A-Za-z0-9]/.test(p)) score++;
  return score;
});
const valid = computed(
  () => username.value && email.value && password.value.length >= 8 && Object.keys(errors.value).length === 0,
);

async function submit() {
  touched.value = true;
  if (!valid.value) return;
  busy.value = true;
  error.value = null;
  fieldError.value = {};
  try {
    await session.register({
      username: username.value.trim(),
      email: email.value.trim(),
      password: password.value,
      displayName: displayName.value.trim() || undefined,
    });
    void router.replace("/");
  } catch (e) {
    if (e instanceof ApiError && e.code === "username_taken") fieldError.value = { username: e.message };
    else if (e instanceof ApiError && e.code === "email_taken") fieldError.value = { email: e.message };
    else error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <AuthLayout>
    <form class="auth-form" @submit.prevent="submit">
      <header>
        <p class="auth-form__eyebrow">Únete a Warlay</p>
        <h2>Crear cuenta</h2>
      </header>
      <FormField label="Usuario" for="username" :error="errors.username" hint="Así te encontrarán tus amigos.">
        <WInput
          id="username"
          v-model="username"
          size="lg"
          autocomplete="username"
          placeholder="thrall"
          :invalid="!!errors.username"
          @input="fieldError = {}"
        >
          <template #icon><AtSign /></template>
        </WInput>
      </FormField>
      <FormField label="Nombre visible (opcional)" for="display-name">
        <WInput id="display-name" v-model="displayName" size="lg" placeholder="Thrall, Hijo de Durotan" :maxlength="32">
          <template #icon><User /></template>
        </WInput>
      </FormField>
      <FormField label="Correo" for="email" :error="errors.email">
        <WInput
          id="email"
          v-model="email"
          type="email"
          size="lg"
          autocomplete="email"
          placeholder="thrall@horda.gg"
          :invalid="!!errors.email"
          @input="fieldError = {}"
        >
          <template #icon><Mail /></template>
        </WInput>
      </FormField>
      <FormField label="Contraseña" for="new-password" :error="errors.password">
        <WInput
          id="new-password"
          v-model="password"
          type="password"
          size="lg"
          autocomplete="new-password"
          placeholder="Mínimo 8 caracteres"
          :invalid="!!errors.password"
        >
          <template #icon><KeyRound /></template>
        </WInput>
        <div class="auth-form__strength" :data-score="strength" aria-hidden="true">
          <span v-for="n in 4" :key="n" :class="{ on: n <= strength }" />
        </div>
      </FormField>
      <p v-if="error" class="auth-form__error" role="alert">{{ error }}</p>
      <WButton type="submit" size="lg" block :loading="busy" :disabled="!valid">Crear cuenta</WButton>
      <p class="auth-form__switch">¿Ya tienes cuenta? <RouterLink to="/login">Inicia sesión</RouterLink></p>
    </form>
  </AuthLayout>
</template>

<style lang="scss">
.auth-form__strength {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 4px;
  span {
    height: 4px;
    border-radius: 4px;
    background: var(--bg-5);
    transition: background var(--t-med);
  }
  &[data-score="1"] span.on {
    background: var(--danger);
  }
  &[data-score="2"] span.on {
    background: var(--warning);
  }
  &[data-score="3"] span.on,
  &[data-score="4"] span.on {
    background: var(--success);
  }
}
</style>
