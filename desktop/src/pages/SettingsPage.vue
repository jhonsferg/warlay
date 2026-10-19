<script setup lang="ts">
import { Activity, FolderOpen, Gamepad2, KeyRound, Mic, Palette, RefreshCw, Server, Settings, User } from "@lucide/vue";
import { computed, onMounted, onUnmounted, ref } from "vue";
import { auth } from "../services/api";
import { errorMessage, mainServerUrl, setMainServerUrl } from "../services/http";
import { isDesktop, native, type AgentStatus } from "../services/native";
import { realtime } from "../services/realtime";
import { AVATAR_COLORS } from "../ui/palette";
import { useSession } from "../stores/session";
import { useSettings } from "../stores/settings";
import { useToasts } from "../stores/toasts";
import WAvatar from "../ui/atoms/WAvatar.vue";
import WBadge from "../ui/atoms/WBadge.vue";
import WButton from "../ui/atoms/WButton.vue";
import WInput from "../ui/atoms/WInput.vue";
import WSwitch from "../ui/atoms/WSwitch.vue";
import FormField from "../ui/molecules/FormField.vue";
import TabBar from "../ui/molecules/TabBar.vue";
import HomeSidebar from "../ui/organisms/HomeSidebar.vue";
import LogSettingsSection from "../ui/organisms/LogSettingsSection.vue";
import AppShell from "../ui/templates/AppShell.vue";

const session = useSession();
const settings = useSettings();
const toasts = useToasts();

type Tab = "account" | "game" | "network" | "logs";
const tab = ref<Tab>("account");

const displayName = ref(session.me?.displayName ?? "");
const bio = ref(session.me?.bio ?? "");
const color = ref(session.me?.avatarColor ?? AVATAR_COLORS[0]);
const savingProfile = ref(false);

const currentPassword = ref("");
const newPassword = ref("");
const savingPassword = ref(false);

const serverUrl = ref(mainServerUrl());
const agent = ref<AgentStatus | null>(null);
const realtimeVia = ref(realtime.via || "sin conexión");
let agentTimer: ReturnType<typeof setInterval> | null = null;

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

function saveServer() {
  setMainServerUrl(serverUrl.value);
  toasts.info("Servidor actualizado", "Reinicia la sesión para aplicar el cambio.");
}

async function refreshAgent() {
  realtimeVia.value =
    realtime.state === "open"
      ? realtime.via === "principal"
        ? "servidor principal"
        : `ayudante ${realtime.via}`
      : "reconectando...";
  try {
    agent.value = await native.agentStatus();
  } catch {
    agent.value = null;
  }
}

onMounted(() => {
  void settings.load().catch(() => {});
  void refreshAgent();
  agentTimer = setInterval(refreshAgent, 3000);
});
onUnmounted(() => agentTimer && clearInterval(agentTimer));
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
        { id: 'network', label: 'Red y agente' },
        { id: 'logs', label: 'Registros' },
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
        <FormField label="Argumentos" for="args" hint="Por ejemplo: -window -opengl">
          <WInput id="args" v-model="settings.gameArgs" placeholder="-window" />
        </FormField>
        <div class="settings__toggle">
          <Mic />
          <div>
            <strong>Chat de voz en las salas</strong>
            <p>Habla con tus compañeros; la voz viaja directo entre jugadores.</p>
          </div>
          <WSwitch v-model="settings.voiceEnabled" label="Chat de voz en las salas" />
        </div>
        <div class="settings__actions">
          <WButton @click="saveGame">Guardar</WButton>
        </div>
      </section>
    </div>

    <div v-else class="settings__grid">
      <section class="settings__card">
        <h3><Activity /> Agente de red</h3>
        <p class="settings__muted">El agente conecta tu juego directamente con los demás jugadores.</p>
        <p class="settings__muted" data-testid="realtime-via">
          Chat y avisos en tiempo real: <strong>{{ realtimeVia }}</strong>
        </p>
        <div v-if="agent" class="settings__stats">
          <div>
            <span>Activo desde</span><strong>{{ Math.round(agent.uptimeSecs / 60) }} min</strong>
          </div>
          <div>
            <span>Túnel de juego</span><strong>{{ agent.tunnelActive ? "Activo" : "Inactivo" }}</strong>
          </div>
          <div>
            <span>Enlaces</span><strong>{{ agent.meshPeers.length }}</strong>
          </div>
        </div>
        <ul v-if="agent?.meshPeers.length" class="settings__links">
          <li v-for="p in agent.meshPeers" :key="p.clientId">
            <code>{{ p.clientId.slice(0, 8) }}</code>
            <span>{{ p.p2pAddr ?? "—" }}</span>
            <WBadge
              :tone="
                p.state === 'connected'
                  ? 'success'
                  : p.state === 'relayed'
                    ? 'info'
                    : p.state === 'failed'
                      ? 'danger'
                      : 'warning'
              "
            >
              {{
                p.state === "connected"
                  ? "P2P directo"
                  : p.state === "relayed"
                    ? "Vía ayudante"
                    : p.state === "failed"
                      ? "Falló"
                      : "Conectando"
              }}
            </WBadge>
          </li>
        </ul>
        <div class="settings__actions">
          <WButton variant="secondary" @click="native.retryPeerMesh()"
            ><template #icon><RefreshCw /></template>Reintentar enlaces</WButton
          >
        </div>
      </section>

      <section class="settings__card">
        <h3><Server /> Servidor de Warlay</h3>
        <FormField
          label="Dirección del servidor principal"
          for="server-url"
          hint="Solo cámbiala si usas un servidor propio."
        >
          <WInput id="server-url" v-model="serverUrl" placeholder="https://warlay.example.com" />
        </FormField>
        <div class="settings__actions">
          <WButton variant="secondary" @click="saveServer">Aplicar</WButton>
        </div>
      </section>
    </div>

    <LogSettingsSection v-if="tab === 'logs'" />
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
  &__stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--s-2);
    div {
      display: flex;
      flex-direction: column;
      gap: 2px;
      padding: var(--s-3);
      background: var(--bg-2);
      border-radius: var(--r-md);
    }
    span {
      @include eyebrow;
    }
    strong {
      font-size: var(--fs-lg);
    }
  }
  &__links {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    li {
      display: flex;
      align-items: center;
      gap: var(--s-3);
      span {
        flex: 1;
        color: var(--text-3);
        font-size: var(--fs-sm);
      }
    }
    code {
      color: var(--text-2);
    }
  }
}
</style>
