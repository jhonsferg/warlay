<script setup lang="ts">
import { CircleCheck, Download, ExternalLink, RefreshCw, Rocket, TriangleAlert } from "@lucide/vue";
import { computed, onMounted } from "vue";
import { isDesktop, native } from "../../services/native";
import { formatBytes, percent, releaseHighlights } from "../../services/updates";
import { useUpdates } from "../../stores/updates";
import WButton from "../atoms/WButton.vue";

const updates = useUpdates();
const view = computed(() => updates.state);
const status = computed(() => view.value.status);
const phase = computed(() => view.value.phase);
const pct = computed(() => percent(view.value.progress));
const highlights = computed(() => releaseHighlights(status.value?.notes ?? ""));
const busy = computed(() => ["checking", "downloading", "installing"].includes(phase.value));

const publishedOn = computed(() => {
  const at = status.value?.publishedAt;
  return at ? new Date(at).toLocaleDateString("es", { day: "numeric", month: "long", year: "numeric" }) : "";
});

onMounted(() => {
  // Look once when the section opens, like Discord does on start.
  if (isDesktop && phase.value === "idle") void updates.check();
});
</script>

<template>
  <div class="updates">
    <p v-if="!isDesktop" class="updates__muted" data-testid="updates-desktop-only">
      Las actualizaciones se gestionan desde la aplicación de escritorio.
    </p>

    <section v-else class="updates__card" :data-phase="phase">
      <header class="updates__head">
        <span class="updates__icon" :class="`updates__icon--${phase}`">
          <RefreshCw v-if="phase === 'checking' || phase === 'idle'" :class="{ spin: phase === 'checking' }" />
          <CircleCheck v-else-if="phase === 'uptodate'" />
          <Rocket v-else-if="phase === 'available' || phase === 'downloading' || phase === 'installing'" />
          <TriangleAlert v-else />
        </span>
        <div class="updates__title">
          <h3 v-if="phase === 'checking' || phase === 'idle'">Buscando actualizaciones...</h3>
          <h3 v-else-if="phase === 'uptodate'" data-testid="update-title">Warlay está al día</h3>
          <h3 v-else-if="phase === 'downloading'" data-testid="update-title">
            Descargando la versión {{ status?.latest }}
          </h3>
          <h3 v-else-if="phase === 'installing'" data-testid="update-title">Instalando y reiniciando...</h3>
          <h3 v-else-if="phase === 'available'" data-testid="update-title">
            Hay una nueva versión: {{ status?.latest }}
          </h3>
          <h3 v-else data-testid="update-title">No se pudo buscar actualizaciones</h3>
          <p class="updates__muted">
            Versión instalada <strong data-testid="current-version">{{ status?.current ?? "—" }}</strong>
            <template v-if="status && phase !== 'available' && phase !== 'downloading' && phase !== 'installing'">
              · última publicada {{ status.latest }}
            </template>
          </p>
        </div>
      </header>

      <div
        v-if="phase === 'downloading' || phase === 'installing'"
        class="updates__progress"
        data-testid="update-progress"
      >
        <div
          class="updates__bar"
          role="progressbar"
          aria-label="Descarga de la actualización"
          :aria-valuenow="pct ?? undefined"
          aria-valuemin="0"
          aria-valuemax="100"
        >
          <span
            :class="{ indeterminate: pct === null || phase === 'installing' }"
            :style="{ width: `${pct ?? 100}%` }"
          />
        </div>
        <p class="updates__muted">
          <template v-if="phase === 'installing'">Verificada. Warlay se cerrará y volverá a abrirse solo.</template>
          <template v-else>
            {{ pct ?? 0 }}% · {{ formatBytes(view.progress?.receivedBytes ?? 0) }} de
            {{ formatBytes(view.progress?.totalBytes ?? 0) }}
          </template>
        </p>
      </div>

      <template v-if="phase === 'available' && status">
        <p class="updates__muted">Publicada el {{ publishedOn }} · {{ formatBytes(status.sizeBytes) }}</p>
        <ul v-if="highlights.length" class="updates__notes" data-testid="update-notes">
          <li v-for="line in highlights" :key="line">{{ line }}</li>
        </ul>
        <p v-if="!status.canSelfUpdate" class="updates__hint" data-testid="update-manual">
          Esta copia no se instaló con el instalador (es portátil), así que no puede actualizarse sola. Descarga la
          nueva versión desde GitHub.
        </p>
      </template>

      <p v-if="view.error" class="updates__error" role="alert" data-testid="update-error">
        {{ view.error }}
      </p>

      <div class="updates__actions">
        <WButton
          v-if="phase === 'available' && status?.canSelfUpdate"
          size="lg"
          variant="success"
          data-testid="update-install"
          @click="updates.install()"
        >
          <template #icon><Download /></template>Actualizar y reiniciar
        </WButton>
        <WButton
          v-else-if="phase === 'available' && status"
          size="lg"
          data-testid="update-open-page"
          @click="native.openReleasePage(status.pageUrl)"
        >
          <template #icon><ExternalLink /></template>Abrir la página de descarga
        </WButton>
        <WButton
          variant="secondary"
          :disabled="busy"
          :loading="phase === 'checking'"
          data-testid="update-check"
          @click="updates.check()"
        >
          <template #icon><RefreshCw /></template>Buscar actualizaciones
        </WButton>
      </div>
    </section>
  </div>
</template>

<style scoped lang="scss">
.updates {
  max-width: 640px;

  &__card {
    @include card(var(--s-6));
    display: flex;
    flex-direction: column;
    gap: var(--s-4);

    &[data-phase="available"],
    &[data-phase="downloading"],
    &[data-phase="installing"] {
      border-color: rgba(52, 211, 153, 0.4);
      background: radial-gradient(420px 180px at 100% 0%, rgba(52, 211, 153, 0.12), transparent 60%), var(--bg-2);
    }
    &[data-phase="error"] {
      border-color: rgba(239, 68, 68, 0.4);
    }
  }
  &__head {
    display: flex;
    align-items: center;
    gap: var(--s-4);
  }
  &__title h3 {
    margin: 0 0 2px;
  }
  &__icon {
    flex-shrink: 0;
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--bg-4);
    color: var(--text-2);

    &--uptodate,
    &--available,
    &--downloading,
    &--installing {
      background: rgba(52, 211, 153, 0.16);
      color: var(--success, #3ecf8e);
    }
    &--error {
      background: rgba(239, 68, 68, 0.16);
      color: var(--danger, #ef4444);
    }
  }
  &__muted {
    margin: 0;
    color: var(--text-3);
    font-size: var(--fs-sm);
  }
  &__hint {
    margin: 0;
    padding: var(--s-3);
    border-radius: var(--r-sm);
    color: var(--text-2);
    background: var(--bg-4);
    font-size: var(--fs-sm);
  }
  &__error {
    margin: 0;
    color: var(--danger, #ef4444);
    font-size: var(--fs-sm);
  }
  &__notes {
    margin: 0;
    padding-left: var(--s-5);
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--text-2);
    font-size: var(--fs-sm);
  }
  &__progress {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }
  &__bar {
    height: 8px;
    overflow: hidden;
    border-radius: 999px;
    background: var(--bg-4);

    span {
      display: block;
      height: 100%;
      border-radius: inherit;
      background: var(--success, #3ecf8e);
      transition: width 0.15s linear;

      &.indeterminate {
        width: 40% !important;
        animation: updates-slide 1.1s ease-in-out infinite;
      }
    }
  }
  &__actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s-2);
  }
}

.spin {
  animation: updates-spin 1s linear infinite;
}

@keyframes updates-spin {
  to {
    transform: rotate(360deg);
  }
}
@keyframes updates-slide {
  0% {
    margin-left: -40%;
  }
  100% {
    margin-left: 100%;
  }
}

@media (prefers-reduced-motion: reduce) {
  .spin,
  .updates__bar span.indeterminate {
    animation: none;
  }
}
</style>
