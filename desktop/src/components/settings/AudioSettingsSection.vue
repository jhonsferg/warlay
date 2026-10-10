<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { startVoiceChat, stopVoiceChat } from "../../api";
import { useRoomsStore } from "../../store/rooms";
import { useSettingsStore } from "../../store/settings";
import { playClick } from "../../utils/audio";
import BaseCheckbox from "../base/BaseCheckbox.vue";
import BaseInput from "../base/BaseInput.vue";

const settingsStore = useSettingsStore();
const roomsStore = useRoomsStore();

const volumePercent = computed(() => Math.round(settingsStore.settings.masterVolume * 100));

// `setSinkId` (choosing an output device) is a real, shipped Chromium API —
// every WebView2 build (so every Tauri build on Windows) has it — but it's
// still missing from some TS DOM lib targets and isn't universal outside
// Chromium, so it's feature-detected at runtime rather than assumed.
const sinkSupported = typeof HTMLMediaElement !== "undefined" && "setSinkId" in HTMLMediaElement.prototype;
const outputDevices = ref<MediaDeviceInfo[]>([]);
const inputDevices = ref<MediaDeviceInfo[]>([]);

onMounted(async () => {
  if (!navigator.mediaDevices?.enumerateDevices) return;
  try {
    const devices = await navigator.mediaDevices.enumerateDevices();
    if (sinkSupported) {
      outputDevices.value = devices.filter((device) => device.kind === "audiooutput");
    }
    inputDevices.value = devices.filter((device) => device.kind === "audioinput");
  } catch {
    outputDevices.value = [];
    inputDevices.value = [];
  }
});

const outputDeviceId = computed({
  get: () => settingsStore.settings.audioOutputDeviceId,
  set: (value: string) => void settingsStore.setAudioPrefs({ audioOutputDeviceId: value }),
});

const microphoneDeviceId = computed({
  get: () => settingsStore.settings.microphoneDeviceId,
  set: (value: string) => void settingsStore.setVoicePrefs({ microphoneDeviceId: value }),
});

const voiceEnabled = computed({
  get: () => settingsStore.settings.voiceEnabled,
  set: (value: boolean) => {
    void settingsStore.setVoicePrefs({ voiceEnabled: value });
    if (value && roomsStore.activeRoom) {
      void startVoiceChat(
        roomsStore.activeRoom.relayToken,
        settingsStore.settings.microphoneDeviceId || null,
        settingsStore.settings.audioOutputDeviceId || null,
      );
    } else {
      void stopVoiceChat();
    }
  },
});

const micToggleKey = computed({
  get: () => settingsStore.settings.micToggleKey,
  set: (value: string) => void settingsStore.setVoicePrefs({ micToggleKey: value }),
});

function onVolumeInput(event: Event): void {
  const value = Number((event.target as HTMLInputElement).value) / 100;
  void settingsStore.setAudioPrefs({ masterVolume: value });
}

function onVolumeChange(): void {
  playClick();
}

const muted = computed({
  get: () => settingsStore.settings.muted,
  set: (value: boolean) => void settingsStore.setAudioPrefs({ muted: value }),
});
</script>

<template>
  <div class="audio-settings-section">
    <h2>Sonido y Voz</h2>
    <p class="audio-settings-section__note">
      Se aplica de inmediato, sin reiniciar la app — controla el volumen de efectos de la interfaz y la configuración
      del chat de voz P2P.
    </p>

    <label class="row audio-settings-section__mute-row">
      <BaseCheckbox v-model="muted" test-id="settings-audio-muted" aria-label="Silenciar todos los sonidos" />
      Silenciar todos los sonidos de la UI
    </label>

    <label class="field audio-settings-section__volume-field">
      Volumen general — {{ volumePercent }}%
      <input
        type="range"
        min="0"
        max="100"
        step="1"
        :value="volumePercent"
        :disabled="settingsStore.settings.muted"
        data-testid="settings-audio-volume"
        aria-label="Volumen general"
        @input="onVolumeInput"
        @change="onVolumeChange"
      />
    </label>

    <label v-if="sinkSupported" class="field">
      Dispositivo de salida de audio
      <select v-model="outputDeviceId" data-testid="settings-audio-device" aria-label="Dispositivo de salida de audio">
        <option value="">Predeterminado del sistema</option>
        <option v-for="(device, index) in outputDevices" :key="device.deviceId" :value="device.deviceId">
          {{ device.label || `Salida ${index + 1}` }}
        </option>
      </select>
    </label>
    <p v-else class="audio-settings-section__note">
      Este entorno no permite elegir dispositivo de salida — se usa siempre el predeterminado del sistema.
    </p>

    <h3 class="audio-settings-section__subheading">Chat de voz</h3>

    <label class="row audio-settings-section__mute-row">
      <BaseCheckbox v-model="voiceEnabled" test-id="settings-voice-enabled" aria-label="Activar chat de voz" />
      Activar chat de voz en tiempo real
    </label>

    <label class="field">
      Micrófono (Dispositivo de entrada)
      <select
        v-model="microphoneDeviceId"
        :disabled="!voiceEnabled"
        data-testid="settings-voice-input-device"
        aria-label="Micrófono"
      >
        <option value="">Predeterminado del sistema</option>
        <option v-for="(device, index) in inputDevices" :key="device.deviceId" :value="device.deviceId">
          {{ device.label || `Micrófono ${index + 1}` }}
        </option>
      </select>
    </label>

    <div class="field">
      <label for="mic-toggle-key-input">Tecla para encender/apagar micrófono</label>
      <BaseInput
        id="mic-toggle-key-input"
        v-model="micToggleKey"
        :disabled="!voiceEnabled"
        placeholder="Ej: CommandOrControl+Shift+M, F6"
        test-id="settings-voice-mic-toggle-key"
        aria-label="Tecla para encender/apagar micrófono"
      />
      <p class="audio-settings-section__note">
        El micrófono siempre inicia apagado. Presiona esta combinación para encenderlo, y presiónala de nuevo para
        volver a apagarlo — también puedes hacerlo con el botón del micrófono en el overlay dentro del juego. Formato de
        combinación de teclas global. Ejemplos válidos: <code>CommandOrControl+Shift+M</code>, <code>F6</code>,
        <code>ControlLeft</code>.
      </p>
    </div>
  </div>
</template>

<style scoped lang="scss">
.audio-settings-section {
  display: flex;
  flex-direction: column;
  gap: $spacing-md;

  &__subheading {
    margin-top: $spacing-sm;
    font-size: $text-md;
    font-weight: 600;
    color: $text;
    border-bottom: 1px solid $border-bronze;
    padding-bottom: 4px;
  }

  &__note {
    margin: 0;
    font-size: $text-sm;
    color: $text-muted;

    code {
      color: $text-dim;
      background: rgba(0, 0, 0, 0.3);
      padding: 2px 4px;
      border-radius: $radius-sm;
    }
  }

  &__mute-row {
    gap: $spacing-sm;
    align-items: center;
    cursor: pointer;
  }

  &__volume-field {
    input[type="range"] {
      appearance: none;
      width: 100%;
      height: 4px;
      margin-top: $spacing-sm;
      background: $border-bronze;
      border-radius: $radius-sm;
      cursor: pointer;

      &:disabled {
        opacity: 0.4;
        cursor: not-allowed;
      }

      &::-webkit-slider-thumb {
        appearance: none;
        width: 16px;
        height: 16px;
        border-radius: 50%;
        background: $steel;
        border: 1px solid $border-bronze-dark;
        transition: $transition;

        &:hover {
          box-shadow: 0 0 6px $steel-glow;
        }
      }
    }
  }

  select {
    background: $bg-parchment;
    border: 1px solid $border-bronze;
    color: $text;
    padding: 9px 12px;
    border-radius: $radius-md;
    font-family: $font-body;
    font-size: $text-md;
    transition: $transition;

    &:disabled {
      opacity: 0.4;
      cursor: not-allowed;
    }

    &:focus {
      outline: none;
      border-color: $gold;
      box-shadow: $focus-ring;
      background: rgba(255, 255, 255, 0.1);
    }
  }
}
</style>
