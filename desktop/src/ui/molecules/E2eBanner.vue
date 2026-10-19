<script setup lang="ts">
import { Lock, ShieldAlert, ShieldOff } from "@lucide/vue";
import type { ConversationSecurity } from "../../stores/e2e";
import WButton from "../atoms/WButton.vue";

defineProps<{
  security: Exclude<ConversationSecurity, "off">;
  /** The code both sides compare out of band to rule out a swapped key. */
  code?: string;
}>();
const emit = defineEmits<{ trust: [] }>();
</script>

<template>
  <div class="e2e-banner" :class="`e2e-banner--${security}`" role="status" :data-security="security">
    <template v-if="security === 'encrypted'">
      <Lock :size="14" />
      <span>
        Cifrado de extremo a extremo: solo tú y tu amigo pueden leer estos mensajes.
        <span v-if="code" class="e2e-banner__code" title="Compara este código con tu amigo (en persona o por llamada)">
          Código de seguridad <code>{{ code }}</code>
        </span>
      </span>
    </template>
    <template v-else-if="security === 'plain'">
      <ShieldOff :size="14" />
      <span>Esta conversación no está cifrada: tu amigo aún no activó el cifrado de extremo a extremo.</span>
    </template>
    <template v-else>
      <ShieldAlert :size="14" />
      <span>
        La clave de seguridad de tu amigo cambió (¿nuevo dispositivo?).
        <span v-if="code" class="e2e-banner__code"
          >Código nuevo <code>{{ code }}</code></span
        >
        Compruébalo con tu amigo antes de confiar.
      </span>
      <WButton size="sm" variant="secondary" data-testid="trust-key" @click="emit('trust')"
        >Confiar en la nueva clave</WButton
      >
    </template>
  </div>
</template>

<style scoped lang="scss">
.e2e-banner {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  margin: var(--s-2) var(--s-4) 0;
  padding: var(--s-2) var(--s-3);
  border-radius: var(--r-sm);
  font-size: var(--fs-xs);
  color: var(--text-2);
  background: rgba(255, 255, 255, 0.04);

  &--encrypted svg {
    color: var(--success, #3ba55c);
  }
  &--plain svg {
    color: var(--text-3);
  }
  &--changed {
    color: var(--text-1);
    background: rgba(250, 166, 26, 0.12);
    border: 1px solid rgba(250, 166, 26, 0.4);
    svg {
      color: var(--warning, #faa61a);
    }
  }

  &__code {
    display: inline-block;
    margin-left: var(--s-1);
    code {
      font-family: var(--font-mono, monospace);
      letter-spacing: 0.04em;
      color: var(--text-1);
    }
  }
}
</style>
