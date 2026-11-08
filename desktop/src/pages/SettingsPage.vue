<script setup lang="ts">
import { FolderOpen, Gamepad2, KeyRound, Palette, Settings, User } from "@lucide/vue";
import { computed, onMounted, ref } from "vue";
import { auth } from "../services/api";
import { errorMessage } from "../services/http";
import { isDesktop, native } from "../services/native";
import { AVATAR_COLORS } from "../ui/palette";
import { useSession } from "../stores/session";
import { useSettings } from "../stores/settings";
import { useToasts } from "../stores/toasts";
import WAvatar from "../ui/atoms/WAvatar.vue";
import WButton from "../ui/atoms/WButton.vue";
import WInput from "../ui/atoms/WInput.vue";

import FormField from "../ui/molecules/FormField.vue";
import TabBar from "../ui/molecules/TabBar.vue";
import AdvancedSettingsSection from "../ui/organisms/AdvancedSettingsSection.vue";
import GameArgsSwitches from "../ui/organisms/GameArgsSwitches.vue";
import HomeSidebar from "../ui/organisms/HomeSidebar.vue";
import VoiceSettingsSection from "../ui/organisms/VoiceSettingsSection.vue";
import AppShell from "../ui/templates/AppShell.vue";

const session = useSession();
const settings = useSettings();
const toasts = useToasts();

type Tab = "account" | "game" | "advanced";
const tab = ref<Tab>("account");

const displayName = ref(session.me?.displayName ?? "");
const bio = ref(session.me?.bio ?? "");
const color = ref(session.me?.avatarColor ?? AVATAR_COLORS[0]);
const savingProfile = ref(false);

const currentPassword = ref("");
const newPassword = ref("");
const savingPassword = ref(false);

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

async function changePassword() {
  savingPassword.value = true;
  try {
    await auth.changePassword(currentPassword.value, newPassword.value);
    currentPassword.value = "";
    newPassword.value = "";
    toasts.success("Contraseña actualizada", "Las demás sesiones deberán iniciar sesión de nuevo.");
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
        <h3><KeyRound /> Contraseña</h3>
        <FormField label="Contraseña actual" for="current-password">
          <WInput id="current-password" v-model="currentPassword" type="password" autocomplete="current-password" />
        </FormField>
        <FormField label="Nueva contraseña" for="new-password-settings" hint="Al menos 8 caracteres.">
          <WInput id="new-password-settings" v-model="newPassword" type="password" autocomplete="new-password" />
        </FormField>
        <div class="settings__actions">
          <WButton
            variant="secondary"
            :disabled="!currentPassword || newPassword.length < 8"
            :loading="savingPassword"
            @click="changePassword"
          >
            Cambiar contraseña
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
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: var(--s-5);
    align-items: start;
    max-width: 1100px;
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
