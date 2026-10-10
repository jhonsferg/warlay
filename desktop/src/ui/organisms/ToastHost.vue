<script setup lang="ts">
import { CheckCircle2, Info, TriangleAlert } from "@lucide/vue";
import { useToasts } from "../../stores/toasts";

const toasts = useToasts();
</script>

<template>
  <div class="toast-host" aria-live="polite">
    <TransitionGroup name="toast">
      <div
        v-for="t in toasts.items"
        :key="t.id"
        class="toast"
        :class="`toast--${t.tone}`"
        role="status"
        @click="toasts.dismiss(t.id)"
      >
        <CheckCircle2 v-if="t.tone === 'success'" class="toast__icon" />
        <TriangleAlert v-else-if="t.tone === 'error'" class="toast__icon" />
        <Info v-else class="toast__icon" />
        <div>
          <strong>{{ t.title }}</strong>
          <p v-if="t.text">{{ t.text }}</p>
        </div>
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
