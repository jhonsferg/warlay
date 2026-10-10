<script setup lang="ts" generic="T extends string">
const model = defineModel<T>({ required: true });
defineProps<{ tabs: { id: T; label: string; count?: number }[] }>();
</script>

<template>
  <div class="tab-bar" role="tablist">
    <button
      v-for="tab in tabs"
      :key="tab.id"
      type="button"
      role="tab"
      class="tab-bar__tab"
      :class="{ 'tab-bar__tab--active': model === tab.id }"
      :aria-selected="model === tab.id"
      @click="model = tab.id"
    >
      {{ tab.label }}
      <span v-if="tab.count" class="tab-bar__count">{{ tab.count }}</span>
    </button>
  </div>
</template>

<style scoped lang="scss">
.tab-bar {
  display: inline-flex;
  gap: 2px;
  padding: 3px;
  background: var(--bg-1);
  border: 1px solid var(--line);
  border-radius: var(--r-md);

  &__tab {
    @include reset-button;
    @include interactive;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 var(--s-4);
    border-radius: var(--r-sm);
    white-space: nowrap;
    color: var(--text-2);
    font-weight: 600;
    font-size: var(--fs-sm);

    &:hover {
      color: var(--text-1);
    }
    &--active {
      background: var(--bg-4);
      color: var(--text-1);
      box-shadow: var(--shadow-1);
    }
  }

  &__count {
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: var(--r-full);
    background: var(--danger);
    color: #fff;
    font-size: 10px;
    line-height: 18px;
    text-align: center;
  }
}
</style>
