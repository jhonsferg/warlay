<script setup lang="ts">
import { KeyRound, ShieldCheck, User } from "@lucide/vue";
import { ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ApiError, errorMessage } from "../services/http";
import { useSession } from "../stores/session";
import WButton from "../ui/atoms/WButton.vue";
import WInput from "../ui/atoms/WInput.vue";
import FormField from "../ui/molecules/FormField.vue";
import GoogleSignIn from "../ui/organisms/GoogleSignIn.vue";
import AuthLayout from "../ui/templates/AuthLayout.vue";
import { t } from "../services/i18n";

const session = useSession();
const router = useRouter();
const route = useRoute();
const login = ref("");
const password = ref("");
/** Asked after the password, for accounts with an authenticator app. */
const totpCode = ref("");
const needTotp = ref(false);
const busy = ref(false);
const error = ref<string | null>(null);

function signedInWithGoogle() {
  void router.replace((route.query.next as string) || "/");
}

async function submit() {
  busy.value = true;
  error.value = null;
  try {
    await session.login(login.value.trim(), password.value, needTotp.value ? totpCode.value.trim() : undefined);
    void router.replace((route.query.next as string) || "/");
  } catch (e) {
    if (e instanceof ApiError && e.code === "totp_required") {
      needTotp.value = true;
      error.value = null;
    } else {
      error.value = errorMessage(e);
    }
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <AuthLayout>
    <form class="auth-form" @submit.prevent="submit">
      <header>
        <p class="auth-form__eyebrow">{{ t("Bienvenido de vuelta") }}</p>
        <h2>{{ t("Iniciar sesión") }}</h2>
      </header>
      <FormField :label="t('Usuario o correo')" for="login">
        <WInput
          id="login"
          v-model="login"
          size="lg"
          autocomplete="username"
          :placeholder="t('thrall o thrall@horda.gg')"
        >
          <template #icon><User /></template>
        </WInput>
      </FormField>
      <FormField :label="t('Contraseña')" for="password">
        <WInput
          id="password"
          v-model="password"
          type="password"
          size="lg"
          autocomplete="current-password"
          placeholder="••••••••"
        >
          <template #icon><KeyRound /></template>
        </WInput>
      </FormField>
      <FormField
        v-if="needTotp"
        :label="t('Código de verificación')"
        for="totp"
        :hint="t('El de tu aplicación de autenticación, o un código de recuperación.')"
      >
        <WInput id="totp" v-model="totpCode" size="lg" autocomplete="one-time-code" placeholder="123456">
          <template #icon><ShieldCheck /></template>
        </WInput>
      </FormField>
      <p v-if="error" class="auth-form__error" role="alert">{{ error }}</p>
      <WButton
        type="submit"
        size="lg"
        block
        :loading="busy"
        :disabled="!login.trim() || !password || (needTotp && !totpCode.trim())"
        >{{ t("Entrar") }}</WButton
      >
      <GoogleSignIn @signed-in="signedInWithGoogle" />
      <p class="auth-form__switch">
        <RouterLink to="/recover">{{ t("¿Olvidaste tu contraseña?") }}</RouterLink>
      </p>
      <p class="auth-form__switch">
        {{ t("¿No tienes cuenta?") }} <RouterLink to="/register">{{ t("Crear una") }}</RouterLink>
      </p>
    </form>
  </AuthLayout>
</template>
