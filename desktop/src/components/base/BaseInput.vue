<script setup lang="ts">
withDefaults(defineProps<{ placeholder?: string; type?: string; testId?: string; ariaLabel?: string }>(), {
  placeholder: "",
  type: "text",
  testId: undefined,
  ariaLabel: undefined,
});
const model = defineModel<string>({ default: "" });
</script>

<template>
  <input
    v-model="model"
    class="base-input"
    :type="type"
    :placeholder="placeholder"
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
  width: 100%;
  transition: $transition;

  &::placeholder {
    color: $text-muted;
  }

  // The old theme deliberately kept focus (steel) and action (gold)
  // visually distinct. macOS doesn't make that distinction -- the accent
  // color IS the focus ring on every native text field -- so this now
  // uses the same accent as a primary button, which is the expected
  // Cupertino convention rather than an oversight.
  &:focus {
    outline: none;
    border-color: $gold;
    box-shadow: $focus-ring;
    background: rgba(255, 255, 255, 0.1);
  }
}
</style>
