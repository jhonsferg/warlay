<script setup lang="ts">
import { X } from "@lucide/vue";
import { onMounted, onUnmounted } from "vue";
import WIconButton from "../atoms/WIconButton.vue";

defineProps<{ title: string; subtitle?: string; width?: number }>();
const emit = defineEmits<{ close: [] }>();

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
}
onMounted(() => window.addEventListener("keydown", onKey));
onUnmounted(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <Teleport to="body">
    <div class="w-modal" @mousedown.self="emit('close')">
      <section
        class="w-modal__panel"
        role="dialog"
        aria-modal="true"
        :aria-label="title"
        :style="{ maxWidth: `${width ?? 460}px` }"
      >
        <header class="w-modal__head">
          <div>
            <h2>{{ title }}</h2>
            <p v-if="subtitle">{{ subtitle }}</p>
          </div>
          <WIconButton label="Cerrar" size="sm" @click="emit('close')"><X /></WIconButton>
        </header>
        <div class="w-modal__body"><slot /></div>
        <footer v-if="$slots.footer" class="w-modal__foot"><slot name="footer" /></footer>
      </section>
    </div>
  </Teleport>
</template>

<style scoped lang="scss">
.w-modal {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: grid;
  place-items: center;
  padding: var(--s-6);
  background: rgba(3, 5, 10, 0.66);
  backdrop-filter: blur(6px);
  animation: w-fade var(--t-med) var(--ease);

  &__panel {
    width: 100%;
    background: var(--bg-2);
    border: 1px solid var(--line-2);
    border-radius: var(--r-xl);
    box-shadow: var(--shadow-3);
    animation: w-rise var(--t-med) var(--ease);
    overflow: hidden;
  }

  &__head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--s-4);
    padding: var(--s-6) var(--s-6) var(--s-2);
    h2 {
      margin: 0;
      @include display-font(var(--fs-2xl));
    }
    p {
      margin: 6px 0 0;
      color: var(--text-3);
    }
  }
  &__body {
    padding: var(--s-4) var(--s-6) var(--s-6);
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
  }
  &__foot {
    display: flex;
    justify-content: flex-end;
    gap: var(--s-2);
    padding: var(--s-4) var(--s-6);
    background: var(--bg-1);
    border-top: 1px solid var(--line);
  }
}

@keyframes w-fade {
  from {
    opacity: 0;
  }
}
@keyframes w-rise {
  from {
    opacity: 0;
    transform: translateY(12px) scale(0.98);
  }
}
</style>
