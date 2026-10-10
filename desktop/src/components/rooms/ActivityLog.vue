<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { useRoomsStore } from "../../store/rooms";

const roomsStore = useRoomsStore();
const logEl = ref<HTMLDivElement | null>(null);

watch(
  () => roomsStore.activityLog.length,
  async () => {
    await nextTick();
    if (logEl.value) logEl.value.scrollTop = logEl.value.scrollHeight;
  },
);
</script>

<template>
  <div class="activity-log" ref="logEl" role="log" aria-live="polite" data-testid="activity-log">
    <div v-for="(line, index) in roomsStore.activityLog" :key="index" class="activity-log__line">{{ line }}</div>
  </div>
</template>

<style scoped lang="scss">
.activity-log {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 160px;
  overflow-y: auto;
  padding: $spacing-sm $spacing-md;
  background: rgba(0, 0, 0, 0.25);
  border: 1px solid $border-bronze;
  border-radius: $radius-md;
  font-size: $text-sm;
  color: $text-dim;

  &__line {
    // A quiet neutral marker, not the accent -- a log line isn't an
    // action or a selection, and repeating the accent color on every row
    // would just be noise.
    &::before {
      content: "› ";
      color: $text-muted;
    }
  }
}
</style>
