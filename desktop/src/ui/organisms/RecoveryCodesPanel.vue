<script setup lang="ts">
import { Check, Copy, ShieldAlert } from "@lucide/vue";
import { ref } from "vue";
import { codesText } from "../../services/recoveryCodes";
import WButton from "../atoms/WButton.vue";

const props = defineProps<{ username: string; codes: string[] }>();

const copied = ref(false);

async function copy() {
  try {
    await navigator.clipboard.writeText(codesText(props.username, props.codes));
    copied.value = true;
    setTimeout(() => (copied.value = false), 2500);
  } catch {
    /* the codes stay on screen to be copied by hand */
  }
}
</script>

<template>
  <div class="recovery-codes">
    <p class="recovery-codes__warning" role="note">
      <ShieldAlert :size="16" />
      Guarda estos códigos ahora: no volverán a mostrarse. Cada uno sirve una sola vez para poner una contraseña nueva
      si olvidas la tuya.
    </p>
    <ol class="recovery-codes__list" data-testid="recovery-codes">
      <li v-for="code in codes" :key="code">
        <code>{{ code }}</code>
      </li>
    </ol>
    <WButton variant="secondary" data-testid="copy-recovery-codes" @click="copy">
      <template #icon><Check v-if="copied" /><Copy v-else /></template>
      {{ copied ? "Copiados" : "Copiar los códigos" }}
    </WButton>
  </div>
</template>

<style scoped lang="scss">
.recovery-codes {
  display: flex;
  flex-direction: column;
  gap: var(--s-3);

  &__warning {
    display: flex;
    gap: var(--s-2);
    align-items: flex-start;
    margin: 0;
    padding: var(--s-3);
    font-size: var(--fs-sm);
    color: var(--text-1);
    border-radius: var(--r-sm);
    background: rgba(250, 166, 26, 0.12);
    border: 1px solid rgba(250, 166, 26, 0.4);
    svg {
      flex: none;
      margin-top: 2px;
      color: var(--warning, #faa61a);
    }
  }

  &__list {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: var(--s-2);
    margin: 0;
    padding: 0;
    list-style: none;

    li {
      padding: var(--s-2) var(--s-3);
      text-align: center;
      border-radius: var(--r-sm);
      background: var(--bg-1);
      border: 1px solid var(--line-2);
    }
    code {
      font-family: var(--font-mono, monospace);
      font-size: var(--fs-md);
      letter-spacing: 0.08em;
      color: var(--text-1);
      user-select: all;
    }
  }
}
</style>
