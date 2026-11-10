<script setup lang="ts">
import { FolderOpen, Gamepad2, KeyRound, LogOut, Palette, Settings, ShieldCheck, Trash2, User } from "@lucide/vue";
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { auth } from "../services/api";
import { errorMessage, setTokens } from "../services/http";
import { isDesktop, native } from "../services/native";
import { AVATAR_COLORS } from "../ui/palette";
import { useDialogs } from "../stores/dialogs";
import { useRooms } from "../stores/rooms";
import { useSession } from "../stores/session";
import { useSettings } from "../stores/settings";
import { useToasts } from "../stores/toasts";
import WAvatar from "../ui/atoms/WAvatar.vue";
import WButton from "../ui/atoms/WButton.vue";
import WInput from "../ui/atoms/WInput.vue";

import FormField from "../ui/molecules/FormField.vue";
import TabBar from "../ui/molecules/TabBar.vue";
import AdvancedSettingsSection from "../ui/organisms/AdvancedSettingsSection.vue";
import RecoveryCodesPanel from "../ui/organisms/RecoveryCodesPanel.vue";
import SessionsPanel from "../ui/organisms/SessionsPanel.vue";
import BlockedUsersPanel from "../ui/organisms/BlockedUsersPanel.vue";
import TotpPanel from "../ui/organisms/TotpPanel.vue";
import GameArgsSwitches from "../ui/organisms/GameArgsSwitches.vue";
import HomeSidebar from "../ui/organisms/HomeSidebar.vue";
import VoiceSettingsSection from "../ui/organisms/VoiceSettingsSection.vue";
import AppShell from "../ui/templates/AppShell.vue";

const session = useSession();
const settings = useSettings();
const toasts = useToasts();
const dialogs = useDialogs();
const rooms = useRooms();
const router = useRouter();

type Tab = "account" | "security" | "game" | "advanced";
const route = useRoute();
// A link such as /settings?tab=advanced opens that tab straight away.
const tab = ref<Tab>(
  (["account", "security", "game", "advanced"] as const).find((t) => t === route.query.tab) ?? "account",
);
watch(
  () => route.query.tab,
  (next) => {
    if (next === "account" || next === "security" || next === "game" || next === "advanced") tab.value = next;
  },
);

const displayName = ref(session.me?.displayName ?? "");
const bio = ref(session.me?.bio ?? "");
const color = ref(session.me?.avatarColor ?? AVATAR_COLORS[0]);
const savingProfile = ref(false);

/** False for an account created with Google that has no password yet. */
const hasPassword = computed(() => session.me?.passwordSet !== false);
const currentPassword = ref("");
const newPassword = ref("");
const savingPassword = ref(false);

// Recovery codes: how many are left, and a new set when asked for.
const recoveryRemaining = ref<number | null>(null);
const recoveryPassword = ref("");
const newCodes = ref<string[]>([]);
const generatingCodes = ref(false);

const profileDirty = computed(
  () =>
    displayName.value.trim() !== session.me?.displayName ||
    bio.value !== session.me?.bio ||
    color.value !== session.me?.avatarColor,
);

async function saveProfile() {
  savingProfile.value = true;
  try {
    await session.update({ displayName: displayName.value.trim(), bio: bio.value, avatarColor: color.value });
    toasts.success("Perfil actualizado");
  } catch (e) {
    toasts.error("No se pudo guardar", errorMessage(e));
  } finally {
    savingProfile.value = false;
  }
}

const deleteConfirm = ref("");
const deletePassword = ref("");
const deletingAccount = ref(false);

async function deleteAccount() {
  const accepted = await dialogs.confirm({
    title: "Eliminar cuenta",
    message: "Esto no se puede deshacer.",
    bullets: [
      "Se borran tu perfil, tus amigos, tus mensajes y tus notificaciones.",
      "Se cierran la sala que alojes y las comunidades que hayas creado.",
      "Tu nombre de usuario y tu correo quedarán libres.",
    ],
    confirmLabel: "Eliminar mi cuenta",
    tone: "danger",
  });
  if (!accepted) return;
  deletingAccount.value = true;
  try {
    await auth.deleteAccount(deleteConfirm.value.trim(), hasPassword.value ? deletePassword.value : undefined);
    setTokens(null);
    void router.push("/login");
  } catch (e) {
    toasts.error("No se pudo eliminar la cuenta", errorMessage(e));
  } finally {
    deletingAccount.value = false;
  }
}

async function logout() {
  const accepted = await dialogs.confirm({
    title: "Cerrar sesión",
    message: "¿Seguro que quieres cerrar sesión?",
    bullets: ["Saldrás de la sala en la que estés.", "Tendrás que iniciar sesión de nuevo para volver a entrar."],
    confirmLabel: "Cerrar sesión",
    tone: "danger",
  });
  if (!accepted) return;
  await rooms.leave().catch(() => {});
  await session.logout();
  void router.push("/login");
}

onMounted(async () => {
  try {
    recoveryRemaining.value = (await auth.recoveryStatus()).remaining;
  } catch {
    /* older servers have no recovery codes: the card just stays empty */
  }
});

async function generateCodes() {
  generatingCodes.value = true;
  try {
    newCodes.value = (await auth.newRecoveryCodes(recoveryPassword.value)).codes;
    recoveryRemaining.value = newCodes.value.length;
    recoveryPassword.value = "";
  } catch (e) {
    toasts.error("No se pudieron generar los códigos", errorMessage(e));
  } finally {
    generatingCodes.value = false;
  }
}

async function changePassword() {
  savingPassword.value = true;
  try {
    const first = !hasPassword.value;
    await auth.changePassword(first ? undefined : currentPassword.value, newPassword.value);
    currentPassword.value = "";
    newPassword.value = "";
    if (session.me) session.me = { ...session.me, passwordSet: true };
    if (first) toasts.success("Contraseña creada", "Ahora también puedes entrar con tu usuario y contraseña.");
    else toasts.success("Contraseña actualizada", "Las demás sesiones deberán iniciar sesión de nuevo.");
  } catch (e) {
    toasts.error("No se pudo cambiar la contraseña", errorMessage(e));
  } finally {
    savingPassword.value = false;
  }
}

async function pickExe() {
  const picked = await native.pickExecutable();
  if (picked) settings.gameExecutablePath = picked;
}

async function saveGame() {
  try {
    await settings.save();
    toasts.success("Ajustes del juego guardados");
  } catch (e) {
    toasts.error("No se pudo guardar", errorMessage(e));
  }
}

onMounted(() => {
  void settings.load().catch(() => {});
});
</script>

<template>
  <AppShell title="Ajustes">
    <template #sidebar><HomeSidebar /></template>
    <template #title-icon><Settings :size="20" /></template>

    <TabBar
      v-model="tab"
      class="settings__tabs"
      :tabs="[
        { id: 'account', label: 'Cuenta' },
        { id: 'security', label: 'Seguridad' },
        { id: 'game', label: 'Juego' },
        { id: 'advanced', label: 'Avanzado' },
      ]"
    />

    <div v-if="tab === 'account'" class="settings__grid">
      <section class="settings__card">
        <h3><User /> Perfil</h3>
        <div class="settings__profile">
          <WAvatar :name="displayName || '?'" :color="color" size="xl" />
          <div>
            <strong>{{ displayName || session.me?.username }}</strong>
            <p>@{{ session.me?.username }} · {{ session.me?.email }}</p>
          </div>
        </div>
        <FormField label="Nombre visible" for="display-name">
          <WInput id="display-name" v-model="displayName" :maxlength="32" />
        </FormField>
        <FormField label="Biografía" for="bio" hint="Hasta 190 caracteres.">
          <textarea id="bio" v-model="bio" class="settings__textarea" rows="3" maxlength="190" />
        </FormField>
        <FormField label="Color" for="colors">
          <div id="colors" class="settings__colors" role="radiogroup" aria-label="Color del avatar">
            <button
              v-for="c in AVATAR_COLORS"
              :key="c"
              type="button"
              role="radio"
              :aria-checked="color === c"
              :aria-label="c"
              :class="{ active: color === c }"
              :style="{ background: c }"
              @click="color = c"
            >
              <Palette v-if="color === c" :size="14" />
            </button>
          </div>
        </FormField>
        <div class="settings__actions">
          <WButton :disabled="!profileDirty" :loading="savingProfile" @click="saveProfile">Guardar perfil</WButton>
        </div>
      </section>

      <section class="settings__card">
        <h3><LogOut /> Sesión</h3>
        <p class="settings__muted">
          Cierra la sesión en este equipo. Tendrás que iniciar sesión de nuevo para volver a entrar.
        </p>
        <div class="settings__actions">
          <WButton variant="danger" data-testid="logout" @click="logout">
            <template #icon><LogOut /></template>Cerrar sesión
          </WButton>
        </div>
      </section>
    </div>

    <div v-else-if="tab === 'security'" class="settings__grid">
      <section class="settings__card">
        <h3><KeyRound /> {{ hasPassword ? "Contraseña" : "Crear una contraseña" }}</h3>
        <p v-if="!hasPassword" class="settings__muted">
          Tu cuenta entra con Google. Si creas una contraseña, también podrás entrar con tu usuario y esa contraseña.
        </p>
        <FormField v-if="hasPassword" label="Contraseña actual" for="current-password">
          <WInput id="current-password" v-model="currentPassword" type="password" autocomplete="current-password" />
        </FormField>
        <FormField label="Nueva contraseña" for="new-password-settings" hint="Al menos 8 caracteres.">
          <WInput id="new-password-settings" v-model="newPassword" type="password" autocomplete="new-password" />
        </FormField>
        <div class="settings__actions">
          <WButton
            variant="secondary"
            :disabled="(hasPassword && !currentPassword) || newPassword.length < 8"
            :loading="savingPassword"
            @click="changePassword"
          >
            {{ hasPassword ? "Cambiar contraseña" : "Crear contraseña" }}
          </WButton>
        </div>
      </section>

      <TotpPanel />

      <section v-if="recoveryRemaining !== null" class="settings__card" data-testid="recovery-card">
        <h3><ShieldCheck /> Códigos de recuperación</h3>
        <p class="settings__muted">
          Sirven para poner una contraseña nueva si olvidas la tuya. Te quedan
          <strong>{{ recoveryRemaining }}</strong> sin usar.
        </p>
        <RecoveryCodesPanel v-if="newCodes.length" :username="session.me?.username ?? ''" :codes="newCodes" />
        <p v-if="!hasPassword" class="settings__muted">
          Crea primero una contraseña para poder generar códigos nuevos.
        </p>
        <FormField
          v-if="hasPassword"
          label="Tu contraseña"
          for="recovery-password"
          hint="Los códigos nuevos reemplazan a los anteriores."
        >
          <WInput id="recovery-password" v-model="recoveryPassword" type="password" autocomplete="current-password" />
        </FormField>
        <div v-if="hasPassword" class="settings__actions">
          <WButton variant="secondary" :disabled="!recoveryPassword" :loading="generatingCodes" @click="generateCodes">
            Generar códigos nuevos
          </WButton>
        </div>
      </section>

      <SessionsPanel />

      <BlockedUsersPanel />

      <section class="settings__card" data-testid="delete-account-card">
        <h3><Trash2 /> Eliminar cuenta</h3>
        <p class="settings__muted">
          Borra tu cuenta y sus datos de forma permanente. Para confirmar, escribe tu nombre de usuario
          <strong>{{ session.me?.username }}</strong
          >{{ hasPassword ? " y tu contraseña" : "" }}.
        </p>
        <FormField label="Tu nombre de usuario" for="delete-confirm">
          <WInput id="delete-confirm" v-model="deleteConfirm" autocomplete="off" />
        </FormField>
        <FormField v-if="hasPassword" label="Tu contraseña" for="delete-password">
          <WInput id="delete-password" v-model="deletePassword" type="password" autocomplete="current-password" />
        </FormField>
        <div class="settings__actions">
          <WButton
            variant="danger"
            data-testid="delete-account"
            :disabled="!deleteConfirm.trim() || (hasPassword && !deletePassword)"
            :loading="deletingAccount"
            @click="deleteAccount"
          >
            <template #icon><Trash2 /></template>Eliminar mi cuenta
          </WButton>
        </div>
      </section>
    </div>

    <div v-else-if="tab === 'game'" class="settings__grid">
      <section class="settings__card">
        <h3><Gamepad2 /> Warcraft III</h3>
        <FormField label="Ejecutable" for="exe" hint="Warcraft III.exe o Frozen Throne.exe">
          <div class="settings__row">
            <WInput
              id="exe"
              v-model="settings.gameExecutablePath"
              placeholder="C:\\Juegos\\Warcraft III\\Frozen Throne.exe"
            />
            <WButton v-if="isDesktop" variant="secondary" @click="pickExe"
              ><template #icon><FolderOpen /></template>Buscar</WButton
            >
          </div>
        </FormField>
        <GameArgsSwitches v-model="settings.gameArgs" />
        <div class="settings__actions">
          <WButton @click="saveGame">Guardar</WButton>
        </div>
      </section>

      <VoiceSettingsSection />
    </div>

    <AdvancedSettingsSection v-else-if="tab === 'advanced'" />
  </AppShell>
</template>

<style scoped lang="scss">
.settings {
  &__tabs {
    margin-bottom: var(--s-6);
  }
  &__grid {
    display: grid;
    gap: var(--s-5);
    align-items: start;
    max-width: 1100px;
    margin: 0 auto;
  }
  &__card {
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
  }
  &__profile {
    display: flex;
    align-items: center;
    gap: var(--s-4);
    strong {
      @include display-font(var(--fs-2xl));
    }
    p {
      margin: 4px 0 0;
      color: var(--text-3);
    }
  }
  &__textarea {
    resize: vertical;
    padding: var(--s-3);
    background: var(--bg-1);
    border: 1px solid var(--line-2);
    border-radius: var(--r-sm);
    &:focus {
      outline: none;
      border-color: var(--accent);
      box-shadow: 0 0 0 3px var(--accent-soft);
    }
  }
  &__colors {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s-2);
    button {
      @include reset-button;
      @include interactive;
      width: 30px;
      height: 30px;
      border-radius: 50%;
      display: grid;
      place-items: center;
      color: #fff;
      box-shadow: inset 0 0 0 2px rgba(255, 255, 255, 0.15);
      &:hover {
        transform: scale(1.08);
      }
      &.active {
        box-shadow:
          0 0 0 2px var(--bg-3),
          0 0 0 4px #fff;
      }
    }
  }
  &__row {
    display: flex;
    gap: var(--s-2);
    > :first-child {
      flex: 1;
    }
  }
  &__actions {
    display: flex;
    justify-content: flex-end;
  }
  &__muted {
    margin: 0;
    color: var(--text-3);
  }
  &__toggle {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: var(--s-3) var(--s-4);
    background: var(--bg-2);
    border-radius: var(--r-md);
    color: var(--text-3);
    > svg {
      width: 18px;
      height: 18px;
      flex-shrink: 0;
    }
    div {
      flex: 1;
    }
    strong {
      color: var(--text-1);
      font-weight: 600;
    }
    p {
      margin: 2px 0 0;
      font-size: var(--fs-sm);
    }
  }
}
</style>
