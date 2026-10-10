<script setup lang="ts">
withDefaults(defineProps<{ size?: number; wordmark?: boolean }>(), { size: 36, wordmark: false });
</script>

<template>
  <span class="w-logo">
    <svg :width="size" :height="size" viewBox="0 0 48 48" aria-hidden="true">
      <defs>
        <linearGradient id="w-logo-grad" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stop-color="#7b61ff" />
          <stop offset="0.55" stop-color="#4f7bff" />
          <stop offset="1" stop-color="#22d3ee" />
        </linearGradient>
      </defs>
      <rect x="2" y="2" width="44" height="44" rx="14" fill="url(#w-logo-grad)" />
      <path
        d="M11 15 L17 33 L24 20 L31 33 L37 15"
        fill="none"
        stroke="#fff"
        stroke-width="4.2"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
    <span v-if="wordmark" class="w-logo__word">WARLAY</span>
  </span>
</template>

<style scoped lang="scss">
.w-logo {
  display: inline-flex;
  align-items: center;
  gap: var(--s-3);

  &__word {
    @include display-font(var(--fs-2xl));
    letter-spacing: 0.16em;
  }
}
</style>
