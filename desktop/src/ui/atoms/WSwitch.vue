<script setup lang="ts">
const model = defineModel<boolean>({ default: false });
defineProps<{ label: string }>();
</script>

<template>
  <button
    type="button"
    role="switch"
    class="w-switch"
    :class="{ 'w-switch--on': model }"
    :aria-checked="model"
    :aria-label="label"
    @click="model = !model"
  >
    <span class="w-switch__knob" />
  </button>
</template>

<style scoped lang="scss">
.w-switch {
  @include reset-button;
  @include interactive;
  position: relative;
  width: 40px;
  height: 22px;
  border-radius: var(--r-full);
  background: var(--bg-5);
  flex-shrink: 0;

  &__knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    box-shadow: var(--shadow-1);
    transition: transform var(--t-med) var(--ease);
  }

  &--on {
    background: var(--accent);
  }
  &--on &__knob {
    transform: translateX(18px);
  }
}
</style>
