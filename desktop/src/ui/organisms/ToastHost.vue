<script setup lang="ts">
import { CheckCircle2, Info, TriangleAlert, X } from "@lucide/vue";
import type { Toast, ToastAction } from "../../stores/toasts";
import { useToasts } from "../../stores/toasts";
import WButton from "../atoms/WButton.vue";
import WIconButton from "../atoms/WIconButton.vue";
import { t } from "../../services/i18n";

const toasts = useToasts();

/** Runs an action and closes the toast either way: it has been answered. */
async function perform(toast: Toast, action: ToastAction) {
  try {
    await action.run();
  } finally {
    toasts.dismiss(toast.id);
  }
}
</script>

<template>
  <div class="toast-host" aria-live="polite">
    <TransitionGroup name="toast">
      <div
        v-for="toast in toasts.items"
        :key="toast.id"
        class="toast"
        :class="`toast--${toast.tone}`"
        role="status"
        :data-toast="toast.title"
        @click="!toast.actions && toasts.dismiss(toast.id)"
      >
        <CheckCircle2 v-if="toast.tone === 'success'" class="toast__icon" />
        <TriangleAlert v-else-if="toast.tone === 'error'" class="toast__icon" />
        <Info v-else class="toast__icon" />
        <div class="toast__body">
          <strong>{{ toast.title }}</strong>
          <p v-if="toast.text">{{ toast.text }}</p>
          <div v-if="toast.actions" class="toast__actions">
            <WButton
              v-for="a in toast.actions"
              :key="a.label"
              size="sm"
              :variant="a.variant ?? 'primary'"
              @click.stop="perform(toast, a)"
              >{{ a.label }}</WButton
            >
          </div>
        </div>
        <WIconButton
          v-if="toast.actions"
          class="toast__close"
          :label="t('Cerrar aviso')"
          size="sm"
          data-testid="toast-close"
          @click.stop="toasts.dismiss(toast.id)"
          ><X
        /></WIconButton>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped lang="scss">
.toast-host {
  position: fixed;
  right: var(--s-6);
  bottom: var(--s-6);
  z-index: 60;
  display: flex;
  flex-direction: column;
  gap: var(--s-2);
  width: 340px;
}

.toast {
  display: flex;
  gap: var(--s-3);
  padding: var(--s-3) var(--s-4);
  background: var(--glass);
  backdrop-filter: blur(14px);
  border: 1px solid var(--line-2);
  border-left: 3px solid var(--info);
  border-radius: var(--r-md);
  box-shadow: var(--shadow-2);
  cursor: pointer;

  strong {
    font-weight: 600;
  }
  p {
    margin: 2px 0 0;
    color: var(--text-2);
    font-size: var(--fs-sm);
  }

  &__body {
    flex: 1;
    min-width: 0;
  }
  &__actions {
    display: flex;
    gap: var(--s-2);
    margin-top: var(--s-2);
  }
  &__close {
    flex-shrink: 0;
    align-self: flex-start;
  }

  &__icon {
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    margin-top: 1px;
    color: var(--info);
  }
  &--success {
    border-left-color: var(--success);
  }
  &--success &__icon {
    color: var(--success);
  }
  &--error {
    border-left-color: var(--danger);
  }
  &--error &__icon {
    color: var(--danger);
  }
}

.toast-enter-active,
.toast-leave-active {
  transition: all var(--t-med) var(--ease);
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateX(24px);
}
</style>
