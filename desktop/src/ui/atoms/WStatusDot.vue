<script setup lang="ts">
import { computed } from "vue";
import type { PresenceStatus } from "../../services/types";
import { presenceLabel } from "../presence";

const props = withDefaults(defineProps<{ status: PresenceStatus; size?: "sm" | "md" }>(), { size: "md" });
const label = computed(() => presenceLabel(props.status));
</script>

<template>
  <span class="w-dot" :class="[`w-dot--${status}`, `w-dot--${size}`]" :title="label" :aria-label="label" role="img" />
</template>

<style scoped lang="scss">
.w-dot {
  display: inline-block;
  border-radius: 50%;
  border: 3px solid var(--dot-ring, var(--bg-2));
  box-sizing: content-box;

  &--sm {
    width: 8px;
    height: 8px;
    border-width: 2px;
  }
  &--md {
    width: 10px;
    height: 10px;
  }

  &--online {
    background: var(--online);
  }
  &--in_room {
    background: var(--in-room);
  }
  &--in_game {
    background: var(--in-game);
    box-shadow: 0 0 8px var(--in-game);
  }
  &--offline {
    background: var(--offline);
  }
}
</style>
