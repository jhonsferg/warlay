<script setup lang="ts">
defineProps<{ label: string; for?: string; hint?: string; error?: string | null }>();
</script>

<template>
  <div class="form-field">
    <label class="form-field__label" :for="$props.for">{{ label }}</label>
    <slot />
    <p v-if="error" class="form-field__error" role="alert">{{ error }}</p>
    <p v-else-if="hint" class="form-field__hint">{{ hint }}</p>
  </div>
</template>

<style scoped lang="scss">
.form-field {
  display: flex;
  flex-direction: column;
  gap: 6px;

  &__label {
    @include eyebrow;
    color: var(--text-2);
  }
  &__hint,
  &__error {
    margin: 0;
    font-size: var(--fs-sm);
  }
  &__hint {
    color: var(--text-3);
  }
  &__error {
    color: var(--danger);
  }
}
</style>
