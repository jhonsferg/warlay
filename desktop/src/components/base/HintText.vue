<script setup lang="ts">
withDefaults(defineProps<{ text?: string | null; error?: boolean; testId?: string }>(), {
  text: "",
  error: false,
  testId: undefined,
});
</script>

<template>
  <p
    v-if="text"
    class="hint-text"
    :class="{ 'hint-text--error': error }"
    :data-testid="testId"
    :role="error ? 'alert' : undefined"
  >
    {{ text }}
  </p>
</template>

<style scoped lang="scss">
.hint-text {
  margin: 4px 0 0;
  font-size: $text-sm;
  color: $text-muted;

  &--error {
    color: $danger;
  }
}
</style>
