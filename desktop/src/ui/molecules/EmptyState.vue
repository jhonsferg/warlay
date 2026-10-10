<script setup lang="ts">
defineProps<{ title: string; text?: string }>();
</script>

<template>
  <div class="empty-state">
    <div class="empty-state__art" aria-hidden="true"><slot name="icon" /></div>
    <h3 class="empty-state__title">{{ title }}</h3>
    <p v-if="text" class="empty-state__text">{{ text }}</p>
    <div v-if="$slots.default" class="empty-state__actions"><slot /></div>
  </div>
</template>

<style scoped lang="scss">
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: var(--s-2);
  padding: var(--s-10) var(--s-6);
  color: var(--text-2);

  &__art {
    width: 64px;
    height: 64px;
    border-radius: var(--r-lg);
    display: grid;
    place-items: center;
    background: var(--accent-soft);
    color: #b9aaff;
    margin-bottom: var(--s-2);
    :deep(svg) {
      width: 30px;
      height: 30px;
    }
  }
  &__title {
    margin: 0;
    @include display-font(var(--fs-xl));
    color: var(--text-1);
  }
  &__text {
    margin: 0;
    max-width: 380px;
    color: var(--text-3);
  }
  &__actions {
    margin-top: var(--s-3);
    display: flex;
    gap: var(--s-2);
  }
}
</style>
