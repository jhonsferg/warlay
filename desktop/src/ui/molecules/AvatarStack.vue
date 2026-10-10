<script setup lang="ts">
import { computed } from "vue";
import type { UserSummary } from "../../services/types";
import WAvatar from "../atoms/WAvatar.vue";

const props = withDefaults(defineProps<{ users: UserSummary[]; max?: number; slots?: number }>(), { max: 5, slots: 0 });
const shown = computed(() => props.users.slice(0, props.max));
const hidden = computed(() => Math.max(0, props.users.length - props.max));
// Empty slots read as "seats still open" (closure): the eye completes the row.
const empty = computed(() =>
  Math.max(0, Math.min(props.slots, props.max) - shown.value.length - (hidden.value ? 1 : 0)),
);
</script>

<template>
  <div class="avatar-stack" :aria-label="`${users.length} jugadores`">
    <WAvatar
      v-for="u in shown"
      :key="u.id"
      :name="u.displayName"
      :color="u.avatarColor"
      size="sm"
      class="avatar-stack__item"
    />
    <span v-if="hidden" class="avatar-stack__more">+{{ hidden }}</span>
    <span v-for="n in empty" :key="`empty-${n}`" class="avatar-stack__empty" aria-hidden="true" />
  </div>
</template>

<style scoped lang="scss">
.avatar-stack {
  display: flex;
  align-items: center;

  &__item,
  &__more,
  &__empty {
    margin-left: -8px;
    box-shadow: 0 0 0 2px var(--bg-3);
    border-radius: 50%;
    &:first-child {
      margin-left: 0;
    }
  }

  &__more,
  &__empty {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    font-size: 11px;
    font-weight: 700;
  }
  &__more {
    background: var(--bg-5);
    color: var(--text-2);
  }
  &__empty {
    background: transparent;
    border: 1.5px dashed var(--line-3);
    box-shadow: none;
  }
}
</style>
