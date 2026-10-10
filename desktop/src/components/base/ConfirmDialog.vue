<script setup lang="ts">
import { TriangleAlert } from "@lucide/vue";
import BaseButton from "./BaseButton.vue";
import BaseModal from "./BaseModal.vue";

withDefaults(
  defineProps<{ title: string; message: string; confirmLabel?: string; danger?: boolean; testId?: string }>(),
  { confirmLabel: "Confirmar", danger: false, testId: undefined },
);
const emit = defineEmits<{ confirm: []; cancel: [] }>();
</script>

<template>
  <BaseModal :title="title" :test-id="testId" @close="emit('cancel')">
    <p class="confirm-dialog__message">
      <TriangleAlert v-if="danger" class="confirm-dialog__danger-icon" :size="18" />
      {{ message }}
    </p>
    <div class="confirm-dialog__actions">
      <BaseButton variant="secondary" test-id="confirm-dialog-cancel" @click="emit('cancel')">Cancelar</BaseButton>
      <BaseButton :variant="danger ? 'danger' : 'primary'" test-id="confirm-dialog-confirm" @click="emit('confirm')">{{
        confirmLabel
      }}</BaseButton>
    </div>
  </BaseModal>
</template>

<style scoped lang="scss">
.confirm-dialog {
  &__message {
    display: flex;
    align-items: flex-start;
    gap: $spacing-sm;
    margin: 0 0 $spacing-lg;
    color: $text;
    line-height: 1.5;
  }

  &__danger-icon {
    flex-shrink: 0;
    color: $danger;
    margin-top: 2px;
  }

  &__actions {
    display: flex;
    justify-content: flex-end;
    gap: $spacing-sm;
  }
}
</style>
