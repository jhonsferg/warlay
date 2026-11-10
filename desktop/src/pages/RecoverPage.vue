<script setup lang="ts">
import { CheckCircle2, KeyRound, ShieldCheck, User } from "@lucide/vue";
import { computed, ref } from "vue";
import { auth } from "../services/api";
import { errorMessage } from "../services/http";
import { looksLikeCode } from "../services/recoveryCodes";
import WButton from "../ui/atoms/WButton.vue";
import WInput from "../ui/atoms/WInput.vue";
import FormField from "../ui/molecules/FormField.vue";
import AuthLayout from "../ui/templates/AuthLayout.vue";
import { t } from "../services/i18n";

const login = ref("");
const code = ref("");
const password = ref("");
const repeat = ref("");
const busy = ref(false);
const error = ref<string | null>(null);
/** Codes left after this recovery, once it worked. */
const remaining = ref<number | null>(null);

/** How many codes are left, worded for the player (shown when few remain). */
const left = computed(() => {
  const n = remaining.value ?? 0;
  if (n === 0) return t("No te quedan códigos de recuperación");
  return n === 1 ? t("Te queda 1 código de recuperación") : t("Te quedan {n} códigos de recuperación", { n });
});

const mismatch = computed(() => !!repeat.value && repeat.value !== password.value);
const valid = computed(
  () =>
    login.value.trim() && looksLikeCode(code.value) && password.value.length >= 8 && password.value === repeat.value,
);

async function submit() {
  if (!valid.value) return;
  busy.value = true;
  error.value = null;
  try {
    remaining.value = (await auth.recover(login.value.trim(), code.value.trim(), password.value)).remaining;
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <AuthLayout>
    <div v-if="remaining !== null" class="auth-form" data-testid="recovered">
      <header>
        <p class="auth-form__eyebrow">{{ t("Listo") }}</p>
        <h2><CheckCircle2 :size="22" /> {{ t("Contraseña cambiada") }}</h2>
      </header>
      <p>
        {{ t("Cerramos todas las sesiones abiertas de tu cuenta. Inicia sesión con tu contraseña nueva.") }}
        <template v-if="remaining <= 2">{{
          t("{left}: genera un juego nuevo en Ajustes cuando entres.", { left })
        }}</template>
      </p>
      <RouterLink to="/login" class="auth-form__switch"
        ><WButton size="lg" block>{{ t("Ir a iniciar sesión") }}</WButton></RouterLink
      >
    </div>
    <form v-else class="auth-form" @submit.prevent="submit">
      <header>
        <p class="auth-form__eyebrow">{{ t("Recuperar la cuenta") }}</p>
        <h2>{{ t("Contraseña nueva") }}</h2>
      </header>
      <p class="auth-form__hint">
        {{
          t("Usa uno de los códigos de recuperación que guardaste al crear tu cuenta. Cada código sirve una sola vez.")
        }}
      </p>
      <FormField :label="t('Usuario o correo')" for="recover-login">
        <WInput id="recover-login" v-model="login" size="lg" autocomplete="username" :placeholder="t('thrall')">
          <template #icon><User /></template>
        </WInput>
      </FormField>
      <FormField :label="t('Código de recuperación')" for="recover-code" :hint="t('Formato XXXXX-XXXXX')">
        <WInput id="recover-code" v-model="code" size="lg" autocomplete="off" :placeholder="t('ABCDE-12345')">
          <template #icon><ShieldCheck /></template>
        </WInput>
      </FormField>
      <FormField :label="t('Contraseña nueva')" for="recover-password" :hint="t('Al menos 8 caracteres.')">
        <WInput id="recover-password" v-model="password" type="password" size="lg" autocomplete="new-password">
          <template #icon><KeyRound /></template>
        </WInput>
      </FormField>
      <FormField
        :label="t('Repite la contraseña')"
        for="recover-repeat"
        :error="mismatch ? t('No coinciden') : undefined"
      >
        <WInput
          id="recover-repeat"
          v-model="repeat"
          type="password"
          size="lg"
          autocomplete="new-password"
          :invalid="mismatch"
        >
          <template #icon><KeyRound /></template>
        </WInput>
      </FormField>
      <p v-if="error" class="auth-form__error" role="alert">{{ error }}</p>
      <WButton type="submit" size="lg" block :loading="busy" :disabled="!valid">{{ t("Cambiar contraseña") }}</WButton>
      <p class="auth-form__switch">
        <RouterLink to="/login">{{ t("Volver a iniciar sesión") }}</RouterLink>
      </p>
    </form>
  </AuthLayout>
</template>
