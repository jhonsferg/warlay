<script setup lang="ts">
import { computed, ref, useAttrs } from "vue";

// Layout attributes (class, style) style the wrapper; everything else
// (aria-label, name, ...) belongs on the actual input.
defineOptions({ inheritAttrs: false });
const attrs = useAttrs();
const inputAttrs = computed(() => {
  const { class: _class, style: _style, ...rest } = attrs;
  return rest;
});

const model = defineModel<string>({ default: "" });
const field = ref<HTMLInputElement | null>(null);

withDefaults(
  defineProps<{
    type?: string;
    placeholder?: string;
    invalid?: boolean;
    autocomplete?: string;
    id?: string;
    size?: "md" | "lg";
    maxlength?: number;
  }>(),
  {
    type: "text",
    placeholder: "",
    invalid: false,
    autocomplete: "off",
    id: undefined,
    size: "md",
    maxlength: undefined,
  },
);
</script>

<template>
  <div
    class="w-input"
    :style="attrs.style as never"
    :class="[attrs.class, `w-input--${size}`, { 'w-input--invalid': invalid }]"
    @click="field?.focus()"
  >
    <span v-if="$slots.icon" class="w-input__icon"><slot name="icon" /></span>
    <input
      :id="id"
      ref="field"
      v-model="model"
      class="w-input__field"
      :type="type"
      :placeholder="placeholder"
      :autocomplete="autocomplete"
      :aria-invalid="invalid || undefined"
      :maxlength="maxlength"
      v-bind="inputAttrs"
    />
    <span v-if="$slots.trailing" class="w-input__trailing"><slot name="trailing" /></span>
  </div>
</template>

<style scoped lang="scss">
.w-input {
  @include interactive;
  display: flex;
  align-items: center;
  gap: var(--s-2);
  background: var(--bg-1);
  border: 1px solid var(--line-2);
  border-radius: var(--r-sm);
  padding: 0 var(--s-3);
  cursor: text;

  &--md {
    height: 40px;
  }
  &--lg {
    height: 48px;
    font-size: var(--fs-lg);
  }

  &:hover {
    border-color: var(--line-3);
  }
  &:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  &--invalid {
    border-color: var(--danger);
    &:focus-within {
      box-shadow: 0 0 0 3px var(--danger-soft);
    }
  }

  &__icon,
  &__trailing {
    display: inline-flex;
    color: var(--text-3);
    :deep(svg) {
      width: 17px;
      height: 17px;
    }
  }

  &__field {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text-1);

    &::placeholder {
      color: var(--text-3);
    }
  }
}
</style>
