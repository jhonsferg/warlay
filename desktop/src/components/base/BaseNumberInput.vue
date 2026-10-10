<script setup lang="ts">
withDefaults(defineProps<{ min?: number; max?: number; step?: number; testId?: string; ariaLabel?: string }>(), {
  min: undefined,
  max: undefined,
  step: 1,
  testId: undefined,
  ariaLabel: undefined,
});
const model = defineModel<number>({ default: 0 });
</script>

<template>
  <input
    v-model.number="model"
    class="base-input base-input--number"
    type="number"
    :min="min"
    :max="max"
    :step="step"
    :data-testid="testId"
    :aria-label="ariaLabel"
  />
</template>

<style scoped lang="scss">
.base-input {
  background: $bg-parchment;
  border: 1px solid $border-bronze;
  color: $text;
  padding: 9px 12px;
  border-radius: $radius-md;
  font-family: $font-body;
  font-size: $text-md;
  transition: $transition;

  &--number {
    width: 5rem;
  }

  // Same accent-as-focus-ring convention as BaseInput -- see its comment.
  &:focus {
    outline: none;
    border-color: $gold;
    box-shadow: $focus-ring;
    background: rgba(255, 255, 255, 0.1);
  }
}
</style>
