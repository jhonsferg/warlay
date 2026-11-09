/**
 * Pure helpers behind the microphone settings: which `getUserMedia`
 * constraints a noise level maps to, and what a capture failure means.
 * No DOM access, so it is unit tested under Node.
 */

export type NoiseLevel = "off" | "standard" | "strong";

export function normalizeNoiseLevel(value: unknown): NoiseLevel {
  return value === "off" || value === "strong" ? value : "standard";
}

/** RNNoise runs on top of the capture only at the "strong" level. */
export function usesRnnoise(level: NoiseLevel): boolean {
  return level === "strong";
}

export interface MicPrefs {
  /** `MediaDeviceInfo.deviceId`; "" is the OS default input. */
  deviceId: string;
  noise: NoiseLevel;
  /** Opus packets of 10 ms instead of 20 ms: less delay, more packets. */
  lowLatency?: boolean;
}

/** Length of one Opus packet in microseconds, as WebCodecs wants it. */
export function opusFrameMicros(lowLatency: boolean | undefined): number {
  return lowLatency ? 10000 : 20000;
}

/** What the Opus encoder is configured for (mono, 48 kHz). */
const CHANNELS = 1;
const SAMPLE_RATE = 48000;

export function audioConstraints(prefs: MicPrefs): MediaTrackConstraints {
  return {
    deviceId: prefs.deviceId ? { exact: prefs.deviceId } : undefined,
    channelCount: CHANNELS,
    sampleRate: SAMPLE_RATE,
    // Echo cancellation stays on at every level: with speakers, everyone
    // would otherwise hear themselves back.
    echoCancellation: true,
    autoGainControl: true,
    // "strong" replaces the browser's suppressor with RNNoise; running both
    // would suppress the voice twice and make it sound underwater.
    noiseSuppression: prefs.noise === "standard",
  };
}

export type MicErrorKind = "declined" | "blocked-by-os" | "no-device" | "device-unavailable" | "busy" | "unknown";

export interface MicError {
  kind: MicErrorKind;
  message: string;
}

/**
 * `NotAllowedError` has two very different causes: the user said no to
 * Warlay's own dialog, or -- having said yes -- Windows still blocks desktop
 * apps from the microphone. The stored consent tells them apart.
 */
export function classifyMicError(error: unknown, consentGiven: boolean): MicError {
  const name = typeof error === "object" && error !== null ? String((error as { name?: unknown }).name ?? "") : "";
  switch (name) {
    case "NotAllowedError":
      return consentGiven
        ? {
            kind: "blocked-by-os",
            message:
              "Windows está bloqueando el micrófono para esta aplicación. Actívalo en Configuración > Privacidad > Micrófono.",
          }
        : { kind: "declined", message: "No diste permiso para usar el micrófono." };
    case "NotFoundError":
      return { kind: "no-device", message: "No se encontró ningún micrófono conectado." };
    case "OverconstrainedError":
      return {
        kind: "device-unavailable",
        message: "El micrófono elegido ya no está disponible. Elige otro en Ajustes.",
      };
    case "NotReadableError":
      return { kind: "busy", message: "Otra aplicación está usando el micrófono o no se pudo abrir." };
    default:
      return {
        kind: "unknown",
        message: error instanceof Error ? error.message : "No se pudo activar el micrófono.",
      };
  }
}
