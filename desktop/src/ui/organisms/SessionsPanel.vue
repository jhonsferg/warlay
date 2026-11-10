<script setup lang="ts">
import { Laptop, MonitorSmartphone } from "@lucide/vue";
import { computed, onMounted, ref } from "vue";
import { auth } from "../../services/api";
import { errorMessage } from "../../services/http";
import { agoText, deviceName, methodLabel, type SessionView } from "../../services/sessions";
import { useDialogs } from "../../stores/dialogs";
import { useToasts } from "../../stores/toasts";
import WButton from "../atoms/WButton.vue";
import { useNow } from "../useNow";
import { t } from "../../services/i18n";

/** The devices signed in to the account, with a way to end them. */
const toasts = useToasts();
const dialogs = useDialogs();
const now = useNow();

const sessions = ref<SessionView[]>([]);
const loaded = ref(false);
const busy = ref<string | null>(null);
const others = computed(() => sessions.value.filter((s) => !s.current));

async function load() {
  try {
    sessions.value = await auth.sessions();
  } catch (e) {
    toasts.error(t("No se pudieron cargar las sesiones"), errorMessage(e));
  } finally {
    loaded.value = true;
  }
}

onMounted(load);

async function end(session: SessionView) {
  busy.value = session.id;
  try {
    await auth.endSession(session.id);
    sessions.value = sessions.value.filter((s) => s.id !== session.id);
    toasts.success(t("Sesión cerrada"), deviceName(session.userAgent));
  } catch (e) {
    toasts.error(t("No se pudo cerrar la sesión"), errorMessage(e));
  } finally {
    busy.value = null;
  }
}

async function endOthers() {
  const accepted = await dialogs.confirm({
    title: t("Cerrar las demás sesiones"),
    message: others.value.length === 1 ? t("Se cerrará la sesión en 1 dispositivo.") : t("Se cerrará la sesión en {n} dispositivos.", { n: others.value.length }),
    bullets: [t("Tendrán que iniciar sesión de nuevo."), t("Esta sesión sigue abierta.")],
    confirmLabel: t("Cerrar las demás"),
    tone: "danger",
  });
  if (!accepted) return;
  busy.value = "others";
  try {
    await auth.endOtherSessions();
    sessions.value = sessions.value.filter((s) => s.current);
    toasts.success(t("Las demás sesiones se cerraron"));
  } catch (e) {
    toasts.error(t("No se pudieron cerrar"), errorMessage(e));
  } finally {
    busy.value = null;
  }
}
</script>

<template>
  <section class="sessions" data-testid="sessions-card">
    <h3><MonitorSmartphone /> {{ t("Sesiones activas") }}</h3>
    <p class="sessions__muted">
      {{ t("Los dispositivos donde tu cuenta tiene la sesión abierta. Si no reconoces alguno, ciérralo y cambia tu contraseña.") }}
    </p>
    <p v-if="!loaded" class="sessions__muted">{{ t("Cargando…") }}</p>
    <ul v-else class="sessions__list">
      <li v-for="s in sessions" :key="s.id" :class="{ 'sessions__row--current': s.current }" data-testid="session-row">
        <Laptop class="sessions__icon" :size="20" />
        <div class="sessions__info">
          <strong>{{ deviceName(s.userAgent) }}</strong>
          <span v-if="s.current" class="sessions__badge">{{ t("Esta sesión") }}</span>
          <small>{{ t('{method} · activa {ago}', { method: methodLabel(s.method), ago: agoText(s.lastUsedAt, now) }) }}</small>
        </div>
        <WButton v-if="!s.current" size="sm" variant="secondary" :loading="busy === s.id" @click="end(s)">
          {{ t("Cerrar") }}
        </WButton>
      </li>
    </ul>
    <div v-if="others.length" class="sessions__actions">
      <WButton variant="danger" :loading="busy === 'others'" data-testid="end-other-sessions" @click="endOthers">
        {{ t("Cerrar las demás sesiones") }}
      </WButton>
    </div>
  </section>
</template>

<style scoped lang="scss">
.sessions {
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

  &__list {
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    margin: 0;
    padding: 0;
    list-style: none;

    li {
      display: flex;
      align-items: center;
      gap: var(--s-3);
    }
  }

  &__icon {
    flex: none;
    color: var(--text-3);
  }

  &__info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0 var(--s-2);

    small {
      flex-basis: 100%;
      color: var(--text-3);
    }
  }

  &__badge {
    padding: 1px 8px;
    font-size: var(--fs-xs);
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 999px;
  }

  &__actions {
    display: flex;
    justify-content: flex-end;
  }
}
</style>
