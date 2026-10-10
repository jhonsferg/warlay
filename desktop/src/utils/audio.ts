// Audio synthesis/playback manager for Warlay
import { useSettingsStore } from "../store/settings";

const audios: Record<string, HTMLAudioElement> = {};

function getAudio(name: string): HTMLAudioElement | null {
  try {
    if (!audios[name]) {
      // Usar URL import.meta.url para que Vite resuelva los assets correctamente
      const url = new URL(`../assets/${name}.ogg`, import.meta.url).href;
      audios[name] = new Audio(url);
    }
    return audios[name];
  } catch (err) {
    console.warn(`No se pudo cargar el audio ${name}`, err);
    return null;
  }
}

/** `setSinkId` isn't in every TS DOM lib target yet, even though every
 * Chromium build (so every WebView2 build, so every Tauri build on Windows)
 * has supported it for years — feature-detected at runtime below rather
 * than assumed from the type. */
type SinkCapableAudio = HTMLAudioElement & { setSinkId?(sinkId: string): Promise<void> };

export function playSound(name: string, volume: number = 0.5) {
  // Read the settings store live on every call (not cached at import time)
  // so the Ajustes mute toggle / volume slider / output device take effect
  // on the very next sound played, with no rebuild or reload needed.
  const settingsStore = useSettingsStore();
  if (settingsStore.settings.muted) return;
  const audio = getAudio(name);
  if (audio) {
    // Clonar el nodo permite reproducir sonidos concurrentes sin interrumpirse
    const clone = audio.cloneNode() as SinkCapableAudio;
    clone.volume = Math.max(0, Math.min(1, volume * settingsStore.settings.masterVolume));
    const deviceId = settingsStore.settings.audioOutputDeviceId;
    if (deviceId && clone.setSinkId) {
      // The previously-selected device may have been unplugged since —
      // fall back silently to whatever `play()` does with the default sink
      // rather than losing the sound over a stale deviceId.
      clone.setSinkId(deviceId).catch(() => {});
    }
    clone.play().catch((e) => console.warn("Audio play failed:", e));
  }
}

export function playHover() {
  playSound("hover", 0.2);
}

export function playClick() {
  playSound("click", 0.4);
}

export function playNotification() {
  playSound("notification", 0.5);
}

export function playJoin() {
  playSound("join", 0.5);
}

export function playLeave() {
  playSound("leave", 0.5);
}

export function playConnected() {
  playSound("connected", 0.5);
}

export function playAlert() {
  playSound("alert", 0.5);
}

export function playStartGame() {
  playSound("start", 0.7);
}
