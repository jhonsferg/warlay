<script setup lang="ts">
import { TriangleAlert } from "@lucide/vue";
import { useDialogs } from "../../stores/dialogs";
import WButton from "../atoms/WButton.vue";
import WModal from "./WModal.vue";

/** The one confirmation dialog of the app: `dialogs.confirm({...})` resolves with the answer. */
const dialogs = useDialogs();
</script>

<template>
  <WModal
    v-if="dialogs.confirmRequest"
    :title="dialogs.confirmRequest.title"
    :width="460"
    @close="dialogs.answerConfirm(false)"
  >
    <div class="confirm" data-testid="confirm-dialog" :class="`confirm--${dialogs.confirmRequest.tone}`">
      <TriangleAlert class="confirm__icon" :size="28" />
      <div>
        <p class="confirm__message">{{ dialogs.confirmRequest.message }}</p>
        <ul v-if="dialogs.confirmRequest.bullets?.length">
          <li v-for="b in dialogs.confirmRequest.bullets" :key="b">{{ b }}</li>
        </ul>
      </div>
    </div>
    <template #footer>
      <WButton variant="ghost" data-testid="confirm-cancel" @click="dialogs.answerConfirm(false)">
        {{ dialogs.confirmRequest.cancelLabel }}
      </WButton>
      <WButton
        :variant="dialogs.confirmRequest.tone === 'danger' ? 'danger' : 'primary'"
        data-testid="confirm-accept"
        @click="dialogs.answerConfirm(true)"
      >
        {{ dialogs.confirmRequest.confirmLabel }}
      </WButton>
    </template>
  </WModal>
</template>

<style scoped lang="scss">
.confirm {
  display: flex;
  gap: var(--s-4);
  align-items: flex-start;

  &__icon {
    flex: none;
    color: var(--accent, #5b8def);
  }
  &--danger &__icon {
    color: var(--danger, #ed4245);
  }
  &__message {
    margin: 0;
  }
  ul {
    margin: var(--s-3) 0 0;
    padding-left: var(--s-4);
    display: grid;
    gap: var(--s-2);
    color: var(--text-2);
    font-size: var(--fs-sm);
  }
}
</style>
