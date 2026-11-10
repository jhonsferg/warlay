<script setup lang="ts">
import { onMounted, ref } from "vue";
import { auth } from "../../services/api";
import { isGoogleCancel } from "../../services/googleSignIn";
import { isDesktop, native } from "../../services/native";
import type { AuthProviders } from "../../services/types";
import { useSession } from "../../stores/session";
import WButton from "../atoms/WButton.vue";

const emit = defineEmits<{ signedIn: [] }>();
const session = useSession();

const providers = ref<AuthProviders | null>(null);
const waiting = ref(false);
const error = ref<string | null>(null);

// Only offered where it can work: in the desktop app, on a server that has it.
onMounted(async () => {
  if (!isDesktop) return;
  try {
    const found = await auth.providers();
    providers.value = found.google ? found : null;
  } catch {
    providers.value = null;
  }
});

async function start() {
  if (!providers.value) return;
  waiting.value = true;
  error.value = null;
  try {
    await session.loginWithGoogle(providers.value.webUrl);
    emit("signedIn");
  } catch (e) {
    if (!isGoogleCancel(e)) error.value = e instanceof Error ? e.message : String(e);
  } finally {
    waiting.value = false;
  }
}

function cancel() {
  void native.googleSignInCancel();
}
</script>

<template>
  <div v-if="providers" class="google-signin" data-testid="google-signin">
    <p class="google-signin__divider"><span>o</span></p>
    <div v-if="waiting" class="google-signin__waiting" role="status">
      <p>Termina el inicio de sesión en tu navegador. Warlay sigue aquí esperando.</p>
      <WButton variant="ghost" data-testid="google-cancel" @click="cancel">Cancelar</WButton>
    </div>
    <WButton v-else variant="secondary" size="lg" block data-testid="google-button" @click="start">
      <template #icon>
        <svg viewBox="0 0 18 18" width="18" height="18" aria-hidden="true">
          <path
            fill="#4285F4"
            d="M17.64 9.2c0-.64-.06-1.25-.16-1.84H9v3.48h4.84a4.14 4.14 0 0 1-1.8 2.72v2.26h2.92c1.7-1.57 2.68-3.88 2.68-6.62z"
          />
          <path
            fill="#34A853"
            d="M9 18c2.43 0 4.47-.8 5.96-2.18l-2.92-2.26c-.8.54-1.84.86-3.04.86-2.34 0-4.32-1.58-5.03-3.7H.96v2.33A9 9 0 0 0 9 18z"
          />
          <path
            fill="#FBBC05"
            d="M3.97 10.72A5.4 5.4 0 0 1 3.68 9c0-.6.1-1.18.29-1.72V4.95H.96A9 9 0 0 0 0 9c0 1.45.35 2.83.96 4.05l3.01-2.33z"
          />
          <path
            fill="#EA4335"
            d="M9 3.58c1.32 0 2.5.45 3.44 1.35l2.58-2.58C13.46.89 11.43 0 9 0A9 9 0 0 0 .96 4.95l3.01 2.33C4.68 5.16 6.66 3.58 9 3.58z"
          />
        </svg>
      </template>
      Continuar con Google
    </WButton>
    <p v-if="error" class="google-signin__error" role="alert">{{ error }}</p>
  </div>
</template>

<style scoped lang="scss">
.google-signin {
  display: flex;
  flex-direction: column;
  gap: var(--s-3);

  &__divider {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    margin: 0;
    color: var(--text-3);
    font-size: var(--fs-sm);

    &::before,
    &::after {
      content: "";
      flex: 1;
      height: 1px;
      background: var(--line-2);
    }
  }

  &__waiting {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--s-2);
    text-align: center;
    color: var(--text-2);
    font-size: var(--fs-sm);

    p {
      margin: 0;
    }
  }

  &__error {
    margin: 0;
    color: var(--danger, #ed4245);
    font-size: var(--fs-sm);
    text-align: center;
  }
}
</style>
