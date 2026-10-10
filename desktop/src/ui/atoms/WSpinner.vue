<script setup lang="ts">
withDefaults(defineProps<{ size?: number }>(), { size: 16 });
</script>

<template>
  <span class="w-spinner" :style="{ width: `${size}px`, height: `${size}px` }" role="status" aria-label="Cargando" />
</template>

<style scoped lang="scss">
.w-spinner {
  display: inline-block;
  border-radius: 50%;
  border: 2px solid currentColor;
  border-right-color: transparent;
  opacity: 0.85;
  animation: w-spin 0.7s linear infinite;
}

@keyframes w-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
