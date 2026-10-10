<script setup lang="ts">
defineProps<{ title: string; subtitle?: string; count?: number }>();
</script>

<template>
  <header class="section-header">
    <div>
      <h2 class="section-header__title">
        {{ title }}
        <span v-if="count !== undefined" class="section-header__count">{{ count }}</span>
      </h2>
      <p v-if="subtitle" class="section-header__subtitle">{{ subtitle }}</p>
    </div>
    <div v-if="$slots.default" class="section-header__actions"><slot /></div>
  </header>
</template>

<style scoped lang="scss">
.section-header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: var(--s-4);
  margin-bottom: var(--s-4);

  &__title {
    margin: 0;
    @include display-font(var(--fs-xl));
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: var(--s-2);
  }
  &__count {
    font-family: var(--font-ui);
    font-size: var(--fs-sm);
    font-weight: 700;
    color: var(--text-3);
    background: var(--bg-4);
    border-radius: var(--r-full);
    padding: 1px 8px;
  }
  &__subtitle {
    margin: 4px 0 0;
    color: var(--text-3);
  }
  &__actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--s-2);
    align-items: center;
  }
}
</style>
