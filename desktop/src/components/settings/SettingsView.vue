<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { z } from "zod";
import { getSystemInfo } from "../../api";
import { useSettingsStore } from "../../store/settings";
import type { AppSettings, SystemInfo } from "../../types";
import { debounce } from "../../utils/format";
import { displayNameSchema, gameVersionSchema, slotsTotalSchema } from "../../utils/validation";
import BaseInput from "../base/BaseInput.vue";
import BaseNumberInput from "../base/BaseNumberInput.vue";
import HintText from "../base/HintText.vue";
import AudioSettingsSection from "./AudioSettingsSection.vue";
import ChatSettingsSection from "./ChatSettingsSection.vue";
import ExecutablePicker from "./ExecutablePicker.vue";
import LogLevelSection from "./LogLevelSection.vue";
import AboutModal from "../shared/AboutModal.vue";
import BaseButton from "../base/BaseButton.vue";

const settingsStore = useSettingsStore();
const showAboutModal = ref(false);

// Purely informational (see `commands::system_info::get_system_info`) —
// helps the player decide whether "Motor gráfico" is worth touching at all.
// Never blocks the rest of the view: a query failure just leaves the hint
// showing nothing rather than surfacing an error for a non-essential detail.
const systemInfo = ref<SystemInfo | null>(null);
onMounted(async () => {
  try {
    systemInfo.value = await getSystemInfo();
  } catch {
    systemInfo.value = null;
  }
});
const systemInfoText = computed(() => {
  const info = systemInfo.value;
  if (!info) return "";
  const parts = [`CPU: ${info.cpuCores} núcleos`, `RAM: ${(info.totalRamMb / 1024).toFixed(1)} GB`];
  if (info.gpuName) {
    const vram = info.gpuVramMb ? ` (${(info.gpuVramMb / 1024).toFixed(1)} GB)` : "";
    parts.unshift(`GPU: ${info.gpuName}${vram}`);
  }
  return `Detectado — ${parts.join(" · ")}`;
});

// This view keeps its existing debounced-whole-object-autosave shape rather
// than a submit-based vee-validate form (there's no "submit" here, every
// keystroke eventually saves) — a plain zod schema gates whether `persist()`
// actually calls the API, still letting the user keep typing through an
// invalid intermediate value instead of a submit-form's harder block.
const generalSettingsSchema = z.object({
  displayName: displayNameSchema,
  gameVersion: gameVersionSchema,
  slotsTotal: slotsTotalSchema,
});

// Log levels, log retention, and audio prefs are excluded from this
// draft/autosave path entirely — `LogLevelSection`/`AudioSettingsSection`
// write straight through `settingsStore.setLogLevel`/`setLogRetentionDays`/
// `setAudioPrefs` (instant, no debounce), and `persist()` below re-merges
// those live values from `settingsStore.settings` at send time so an
// in-flight autosave here can't stomp one of those changes made while the
// debounce was still pending with a stale copy of `draft` captured before
// that change happened.
const draft = reactive<AppSettings>({ ...settingsStore.settings });
const hint = ref("");
const hintError = ref(false);
const missingExecutable = computed(() => !settingsStore.settings.gameExecutablePath.trim());

let syncingFromStore = false;

watch(
  () => settingsStore.settings,
  (settings) => {
    syncingFromStore = true;
    Object.assign(draft, settings);
  },
);

const persist = debounce(async () => {
  const validation = generalSettingsSchema.safeParse(draft);
  if (!validation.success) {
    hint.value = validation.error.issues[0]?.message ?? "Revisa los campos marcados.";
    hintError.value = true;
    return;
  }
  try {
    await settingsStore.persistSettings({
      ...draft,
      consoleLogLevel: settingsStore.settings.consoleLogLevel,
      fileLogLevel: settingsStore.settings.fileLogLevel,
      masterVolume: settingsStore.settings.masterVolume,
      muted: settingsStore.settings.muted,
      audioOutputDeviceId: settingsStore.settings.audioOutputDeviceId,
      logRetentionDays: settingsStore.settings.logRetentionDays,
    });
    hint.value = "Guardado.";
    hintError.value = false;
    setTimeout(() => {
      if (hint.value === "Guardado.") hint.value = "";
    }, 1500);
  } catch (error) {
    hint.value = `Error al guardar: ${String(error)}`;
    hintError.value = true;
  }
}, 500);

watch(
  draft,
  () => {
    if (syncingFromStore) {
      syncingFromStore = false;
      return;
    }
    persist();
  },
  { deep: true },
);
</script>

<template>
  <section class="settings-view" data-testid="settings-view">
    <div class="card settings-view__card">
      <h2>Ajustes</h2>
      <HintText
        v-if="settingsStore.pendingGuidance"
        :text="settingsStore.pendingGuidance"
        :error="true"
        test-id="settings-guidance"
      />
      <HintText
        v-if="missingExecutable"
        text="No has seleccionado el ejecutable de Warcraft III (Frozen Throne.exe) — hasta que lo hagas, tendrás que lanzar el juego manualmente al fundar o unirte a una sala."
        :error="true"
        test-id="settings-exe-alert"
      />
      <label class="field"
        >Nombre de guerra<BaseInput
          v-model="draft.displayName"
          placeholder="Thrall"
          test-id="settings-name-input"
          aria-label="Nombre de guerra"
      /></label>

      <ExecutablePicker v-model="draft.gameExecutablePath" />

      <label class="field"
        >Argumentos de lanzamiento<BaseInput
          v-model="draft.gameArgs"
          placeholder="-launch"
          test-id="settings-args-input"
          aria-label="Argumentos de lanzamiento"
      /></label>

      <label class="field"
        >Motor gráfico
        <select v-model="draft.graphicsApi" data-testid="settings-graphics-api" aria-label="Motor gráfico">
          <option value="">Predeterminado (elegido por el juego)</option>
          <option value="Direct3D9">Direct3D 9</option>
          <option value="Direct3D11">Direct3D 11</option>
          <option value="OpenGL2">OpenGL 2</option>
        </select>
      </label>
      <p class="settings-view__note">
        Se agrega como <code>-graphicsapi</code> al lanzar el juego — solo funciona en patches 1.29 o más recientes y
        depende de tu GPU/drivers. Si la partida no abre o se ve mal con una opción, vuelve a "Predeterminado".
      </p>
      <p v-if="systemInfoText" class="settings-view__note" data-testid="settings-system-info">{{ systemInfoText }}</p>

      <div class="row settings-view__row">
        <label class="field"
          >Versión del juego (patch)<BaseNumberInput
            v-model="draft.gameVersion"
            :min="1"
            :max="255"
            test-id="settings-version-input"
            aria-label="Versión del juego"
        /></label>
        <label class="field"
          >Slots totales<BaseNumberInput
            v-model="draft.slotsTotal"
            :min="2"
            :max="24"
            test-id="settings-slots-input"
            aria-label="Slots totales"
        /></label>
      </div>

      <p class="settings-view__note">
        Si tu sala anunciada no aparece o no deja unirse en "Red de Área Local", ajusta la versión para que coincida con
        tu parche de Warcraft III.
      </p>

      <p class="settings-view__note">Los cambios se guardan automáticamente.</p>
      <HintText :text="hint" :error="hintError" test-id="settings-hint" />
    </div>

    <div class="card settings-view__card">
      <ChatSettingsSection v-model="draft.chatMessageLimit" />
    </div>

    <div class="card settings-view__card">
      <AudioSettingsSection />
    </div>

    <div class="card settings-view__card">
      <LogLevelSection />
    </div>

    <div class="card settings-view__card settings-view__about-card">
      <BaseButton variant="secondary" @click="showAboutModal = true"> Acerca de Warlay </BaseButton>
    </div>

    <AboutModal :open="showAboutModal" @close="showAboutModal = false" />
  </section>
</template>

<style scoped lang="scss">
.settings-view {
  // A plain single flex column left a lot of dead space on the right on a
  // maximized/wide window -- every card just piled up under a fixed
  // 560px cap regardless of how much room was actually available. CSS
  // multi-column instead flows cards left-to-right, top-to-bottom across
  // as many ~460px-wide columns as fit (1 on a narrow window, 2-3 on a
  // wide one) -- a masonry-style layout that handles this view's
  // very-uneven card heights (the long "general" card vs. the short
  // "Acerca de" one) better than a CSS grid would, since grid rows would
  // force every card in a row to the same height or leave gaps.
  //
  // Deliberately no height/overflow of its own (that was the bug): a
  // multi-column box with a *constrained* block-size doesn't respond to
  // overflowing content by scrolling that fixed set of columns taller --
  // it spawns additional columns past the box's own width instead, which
  // is what caused the horizontal overflow. Left auto-height, this grows
  // only vertically and scrolls via the ancestor that already does that
  // (.shell__main-content in App.vue).
  column-width: 460px;
  column-gap: $spacing-lg;

  &__card {
    max-width: 560px;
    display: flex;
    flex-direction: column;
    gap: $spacing-md;
    // Multi-column's own gap only applies *between* columns, not between
    // stacked items within one column, and a card must never be split
    // across two columns mid-content.
    break-inside: avoid;
    margin: 0 0 $spacing-lg;
  }

  &__row {
    gap: $spacing-md;
    flex-wrap: wrap;
  }

  &__note {
    margin: 0;
    font-size: $text-sm;
    color: $text-muted;

    code {
      color: $text-dim;
      background: rgba(0, 0, 0, 0.3);
      padding: 1px 4px;
      border-radius: $radius-sm;
    }
  }

  &__about-card {
    align-items: center;
    padding: $spacing-lg;
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

    &:focus {
      outline: none;
      border-color: $gold;
      box-shadow: $focus-ring;
      background: rgba(255, 255, 255, 0.1);
    }
  }
}
</style>
