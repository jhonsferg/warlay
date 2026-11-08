<script setup lang="ts">
import { Mic } from "@lucide/vue";
import { onMounted, onUnmounted, ref } from "vue";
import { type NoiseLevel, normalizeNoiseLevel } from "../../services/voicePrefs";
import { useSettings } from "../../stores/settings";
import { useToasts } from "../../stores/toasts";
import { useVoice } from "../../stores/voice";
import WSwitch from "../atoms/WSwitch.vue";
import FormField from "../molecules/FormField.vue";

/** Input device and noise level for the in-room voice. Applies on its own. */
const settings = useSettings();
const voice = useVoice();
const toasts = useToasts();

interface Input {
  id: string;
  label: string;
}
const inputs = ref<Input[]>([]);
/** Device names are only exposed to a page that already has microphone access. */
const namesHidden = ref(false);

async function refreshDevices() {
  try {
    const all = await navigator.mediaDevices.enumerateDevices();
    const mics = all.filter((d) => d.kind === "audioinput" && d.deviceId !== "default" && d.deviceId !== "communications");
    namesHidden.value = mics.length > 0 && mics.every((d) => !d.label);
    inputs.value = mics.map((d, i) => ({ id: d.deviceId, label: d.label || `Micrófono ${i + 1}` }));
  } catch {
    inputs.value = [];
  }
}

const NOISE_LEVELS: { value: NoiseLevel; label: string }[] = [
  { value: "off", label: "Desactivada" },
  { value: "standard", label: "Estándar (del navegador)" },
  { value: "strong", label: "Fuerte (RNNoise)" },
];

const HINTS: Record<NoiseLevel, string> = {
  off: "Tu micrófono se envía tal cual, solo con cancelación de eco.",
  standard: "Quita ruido de fondo sencillo, como el siseo constante.",
  strong:
    "Usa una red neuronal (RNNoise) para quitar teclado, ventilador y ruidos de fondo. No elimina a otra persona hablando cerca.",
};

async function save(patch: { micDeviceId?: string; noiseSuppression?: NoiseLevel; shareDesktopAudio?: boolean }) {
  try {
    await settings.setVoicePrefs({
      micDeviceId: patch.micDeviceId ?? settings.micDeviceId,
      noiseSuppression: patch.noiseSuppression ?? settings.noiseSuppression,
      shareDesktopAudio: patch.shareDesktopAudio ?? settings.shareDesktopAudio,
    });
    // The PC's sound starts or stops right away when already in voice.
    if (patch.shareDesktopAudio !== undefined) {
      await voice.syncDesktopShare(settings.shareDesktopAudio);
      return;
    }
    if (voice.active) toasts.info("Ajuste guardado", "Se aplicará la próxima vez que te unas a la voz.");
  } catch {
    toasts.error("No se pudo guardar el ajuste de voz");
  }
}

onMounted(() => {
  void refreshDevices();
  navigator.mediaDevices?.addEventListener?.("devicechange", refreshDevices);
});
onUnmounted(() => navigator.mediaDevices?.removeEventListener?.("devicechange", refreshDevices));
</script>

<template>
  <section class="voice-settings" data-testid="voice-settings">
    <h3><Mic /> Voz</h3>

    <FormField label="Micrófono" for="mic-device">
      <select
        id="mic-device"
        class="voice-settings__select"
        data-testid="mic-device"
        :value="settings.micDeviceId"
        @change="save({ micDeviceId: ($event.target as HTMLSelectElement).value })"
      >
        <option value="">Predeterminado del sistema</option>
        <option v-for="d in inputs" :key="d.id" :value="d.id">{{ d.label }}</option>
      </select>
    </FormField>
    <p v-if="namesHidden" class="voice-settings__note" data-testid="mic-names-hidden">
      Los nombres de los dispositivos aparecen después de usar el micrófono una vez.
    </p>

    <FormField label="Supresión de ruido" for="noise-level" :hint="HINTS[normalizeNoiseLevel(settings.noiseSuppression)]">
      <select
        id="noise-level"
        class="voice-settings__select"
        data-testid="noise-level"
        :value="settings.noiseSuppression"
        @change="save({ noiseSuppression: normalizeNoiseLevel(($event.target as HTMLSelectElement).value) })"
      >
        <option v-for="l in NOISE_LEVELS" :key="l.value" :value="l.value">{{ l.label }}</option>
      </select>
    </FormField>

    <div class="voice-settings__share">
      <div>
        <strong>Compartir el audio del escritorio</strong>
        <p class="voice-settings__note" data-testid="share-hint">
          Los jugadores de la sala oirán el sonido de tu PC (tu juego, música, un vídeo) mientras estés en la voz.
          El audio de Warlay, es decir sus voces, nunca se reenvía. Requiere Windows 11.
        </p>
      </div>
      <WSwitch
        :model-value="settings.shareDesktopAudio"
        label="Compartir el audio del escritorio"
        data-testid="share-desktop-audio"
        @update:model-value="save({ shareDesktopAudio: $event })"
      />
    </div>
  </section>
</template>

<style scoped lang="scss">
.voice-settings {
  @include card(var(--s-5));
  display: flex;
  flex-direction: column;
  gap: var(--s-4);

  h3 {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    margin: 0;
  }

  &__note {
    margin: 0;
    color: var(--text-3);
    font-size: var(--fs-sm);
  }

  &__share {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--s-4);
    padding-top: var(--s-3);
    border-top: 1px solid var(--line);
  }

  &__select {
    width: 100%;
    padding: 10px var(--s-3);
    color: var(--text-1);
    background: var(--bg-4);
    border: 1px solid var(--line-2);
    border-radius: var(--r-sm);

    &:focus-visible {
      outline: 2px solid var(--accent);
    }
  }
}
</style>
