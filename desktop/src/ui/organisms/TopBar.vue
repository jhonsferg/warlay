<script setup lang="ts">
import { Gamepad2, PanelRight } from "@lucide/vue";
import { useRouter } from "vue-router";
import { useRooms } from "../../stores/rooms";
import WIconButton from "../atoms/WIconButton.vue";
import NotificationCenter from "./NotificationCenter.vue";

defineProps<{ title: string; subtitle?: string; activityOpen: boolean }>();
defineEmits<{ toggleActivity: [] }>();
const rooms = useRooms();
const router = useRouter();
</script>

<template>
  <header class="top-bar">
    <div class="top-bar__title">
      <slot name="icon" />
      <h1>{{ title }}</h1>
      <span v-if="subtitle" class="top-bar__subtitle">{{ subtitle }}</span>
    </div>
    <div class="top-bar__actions">
      <slot />
      <button
        v-if="rooms.current"
        type="button"
        class="top-bar__room"
        :class="{ 'top-bar__room--ingame': rooms.current.status === 'in_game' }"
        data-testid="current-room-pill"
        @click="router.push(`/room/${rooms.current.id}`)"
      >
        <Gamepad2 :size="15" />
        <span>{{ rooms.current.name }}</span>
        <span class="top-bar__room-count">{{ rooms.current.members.length }}/{{ rooms.current.maxPlayers }}</span>
      </button>
      <NotificationCenter />
      <WIconButton label="Mostrar amigos" :active="activityOpen" @click="$emit('toggleActivity')"
        ><PanelRight
      /></WIconButton>
    </div>
  </header>
</template>

<style scoped lang="scss">
.top-bar {
  height: var(--topbar-h);
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s-4);
  padding: 0 var(--s-5) 0 var(--s-6);
  border-bottom: 1px solid var(--line);
  background: var(--bg-1);

  &__title {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    min-width: 0;
    :deep(svg) {
      color: var(--text-3);
    }
    h1 {
      margin: 0;
      @include display-font(var(--fs-xl));
      @include truncate;
    }
  }
  &__subtitle {
    @include truncate;
    color: var(--text-3);
    font-size: var(--fs-sm);
    padding-left: var(--s-3);
    border-left: 1px solid var(--line-2);
  }

  &__actions {
    display: flex;
    align-items: center;
    gap: var(--s-2);
  }

  &__room {
    @include reset-button;
    @include interactive;
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
    height: 32px;
    padding: 0 var(--s-3);
    border-radius: var(--r-full);
    background: var(--warning-soft);
    color: var(--in-room);
    font-weight: 600;
    font-size: var(--fs-sm);
    max-width: 260px;

    span:not(.top-bar__room-count) {
      @include truncate;
    }
    &:hover {
      filter: brightness(1.15);
    }
    &--ingame {
      background: rgba(244, 114, 182, 0.14);
      color: var(--in-game);
    }
  }
  &__room-count {
    opacity: 0.8;
  }
}
</style>
