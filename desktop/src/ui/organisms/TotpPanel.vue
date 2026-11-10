<script setup lang="ts">
import { ShieldCheck } from "@lucide/vue";
import QRCode from "qrcode";
import { computed, ref } from "vue";
import { auth } from "../../services/api";
import { errorMessage } from "../../services/http";
import type { TotpSetup } from "../../services/types";
import { useSession } from "../../stores/session";
import { useToasts } from "../../stores/toasts";
import WButton from "../atoms/WButton.vue";
import WInput from "../atoms/WInput.vue";
import FormField from "../molecules/FormField.vue";
import { t } from "../../services/i18n";

const session = useSession();
const toasts = useToasts();

const enabled = computed(() => session.me?.totpEnabled === true);
/** Accounts born with Google have no password to ask for. */
const hasPassword = computed(() => session.me?.passwordSet !== false);

const password = ref("");
const code = ref("");
const busy = ref(false);
/** The secret being added to an app, until the first code confirms it. */
const setup = ref<TotpSetup | null>(null);
const qr = ref("");

async function start() {
  busy.value = true;
  try {
    setup.value = await auth.totpSetup(hasPassword.value ? password.value : undefined);
    qr.value = await QRCode.toDataURL(setup.value.otpauthUri, { margin: 1, width: 192 });
    password.value = "";
  } catch (e) {
    toasts.error(t("No se pudo preparar la verificación en dos pasos"), errorMessage(e));
  } finally {
    busy.value = false;
  }
}

async function confirm() {
  busy.value = true;
  try {
    await auth.totpEnable(code.value.trim());
    if (session.me) session.me = { ...session.me, totpEnabled: true };
    setup.value = null;
    qr.value = "";
    code.value = "";
    toasts.success(t("Verificación en dos pasos activada"), t("Al iniciar sesión te pediremos un código de tu aplicación."));
  } catch (e) {
    toasts.error(t("El código no es correcto"), errorMessage(e));
  } finally {
    busy.value = false;
  }
}

async function turnOff() {
  busy.value = true;
  try {
    await auth.totpDisable(code.value.trim(), hasPassword.value ? password.value : undefined);
    if (session.me) session.me = { ...session.me, totpEnabled: false };
    password.value = "";
    code.value = "";
    toasts.success(t("Verificación en dos pasos desactivada"));
  } catch (e) {
    toasts.error(t("No se pudo desactivar"), errorMessage(e));
  } finally {
    busy.value = false;
  }
}

function cancel() {
  setup.value = null;
  qr.value = "";
  code.value = "";
}
</script>

<template>
  <section class="totp" data-testid="totp-card">
    <h3><ShieldCheck /> {{ t("Verificación en dos pasos") }}</h3>

    <template v-if="setup">
      <p class="totp__muted">
        {{ t("Escanea el código con tu aplicación de autenticación (Google Authenticator, Authy, 2FAS, Aegis...) o escribe la clave a mano. Después escribe el código de 6 dígitos que muestre.") }}
      </p>
      <div class="totp__setup">
        <img :src="qr" :alt="t('Código QR para tu aplicación de autenticación')" width="192" height="192" />
        <code class="totp__secret" data-testid="totp-secret">{{ setup.secret }}</code>
      </div>
      <FormField :label="t('Código de 6 dígitos')" for="totp-confirm">
        <WInput id="totp-confirm" v-model="code" autocomplete="one-time-code" placeholder="123456" :maxlength="7" />
      </FormField>
      <div class="totp__actions">
        <WButton variant="ghost" @click="cancel">{{ t("Cancelar") }}</WButton>
        <WButton :disabled="code.trim().length < 6" :loading="busy" data-testid="totp-confirm" @click="confirm">
          {{ t("Activar") }}
        </WButton>
      </div>
    </template>

    <template v-else-if="enabled">
      <p class="totp__muted">
        {{ t("Está") }} <strong>{{ t("activada") }}</strong>{{ t(": al iniciar sesión, además de la contraseña o Google, se pide un código de tu aplicación. Si pierdes el teléfono, un código de recuperación sirve en su lugar (una vez).") }}
      </p>
      <FormField v-if="hasPassword" :label="t('Tu contraseña')" for="totp-off-password">
        <WInput id="totp-off-password" v-model="password" type="password" autocomplete="current-password" />
      </FormField>
      <FormField :label="t('Un código de tu aplicación (o de recuperación)')" for="totp-off-code">
        <WInput id="totp-off-code" v-model="code" autocomplete="one-time-code" />
      </FormField>
      <div class="totp__actions">
        <WButton
          variant="danger"
          :disabled="!code.trim() || (hasPassword && !password)"
          :loading="busy"
          data-testid="totp-off"
          @click="turnOff"
        >
          {{ t("Desactivar") }}
        </WButton>
      </div>
    </template>

    <template v-else>
      <p class="totp__muted">
        {{ t("Protege tu cuenta con un código que cambia cada 30 segundos y que solo muestra tu teléfono. Sirve también para quien entra con Google.") }}
      </p>
      <FormField v-if="hasPassword" :label="t('Tu contraseña')" for="totp-on-password">
        <WInput id="totp-on-password" v-model="password" type="password" autocomplete="current-password" />
      </FormField>
      <div class="totp__actions">
        <WButton :disabled="hasPassword && !password" :loading="busy" data-testid="totp-start" @click="start">
          {{ t("Activar") }}
        </WButton>
      </div>
    </template>
  </section>
</template>

<style scoped lang="scss">
.totp {
  @include card(var(--s-6));
  display: flex;
  flex-direction: column;
  gap: var(--s-4);

  h3 {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    margin: 0;
    @include display-font(var(--fs-xl));
    svg {
      width: 20px;
      height: 20px;
      color: var(--accent);
    }
  }

  &__muted {
    margin: 0;
    color: var(--text-3);
  }

  &__setup {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s-4);

    img {
      border-radius: var(--r-sm);
      background: #fff;
    }
  }

  &__secret {
    font-family: var(--font-mono, monospace);
    letter-spacing: 0.08em;
    word-break: break-all;
    user-select: all;
    color: var(--text-1);
  }

  &__actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--s-2);
  }
}
</style>
