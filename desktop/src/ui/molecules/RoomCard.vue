<script setup lang="ts">
import { Crown, Lock, Users } from "@lucide/vue";
import { computed } from "vue";
import type { RoomView } from "../../services/types";
import WBadge from "../atoms/WBadge.vue";
import WButton from "../atoms/WButton.vue";
import AvatarStack from "./AvatarStack.vue";

const props = defineProps<{ room: RoomView; current?: boolean; busy?: boolean }>();
defineEmits<{ join: [room: RoomView]; open: [room: RoomView] }>();

const full = computed(() => props.room.members.length >= props.room.maxPlayers);
const fill = computed(() => Math.round((props.room.members.length / props.room.maxPlayers) * 100));
</script>

<template>
  <article
    class="room-card"
    :class="{ 'room-card--current': current, 'room-card--ingame': room.status === 'in_game' }"
    :data-room="room.name"
  >
    <header class="room-card__head">
      <div class="room-card__title">
        <h3>{{ room.name }}</h3>
        <span class="room-card__host"><Crown :size="12" /> {{ room.host.displayName }}</span>
      </div>
      <WBadge v-if="room.status === 'in_game'" tone="game" dot>En partida</WBadge>
      <WBadge v-else tone="success" dot>Abierta</WBadge>
    </header>
    <WBadge v-if="room.hostAway" tone="danger" dot>Anfitrión desconectado</WBadge>

    <div class="room-card__tags">
      <WBadge v-if="room.communityName" tone="accent">{{ room.communityName }}</WBadge>
      <WBadge v-else tone="neutral">Pública</WBadge>
      <WBadge v-if="room.hasPassword" tone="warning"><Lock /> Privada</WBadge>
    </div>

    <footer class="room-card__foot">
      <div class="room-card__players">
        <AvatarStack :users="room.members" :max="5" :slots="Math.min(room.maxPlayers, 5)" />
        <span class="room-card__count"><Users :size="14" /> {{ room.members.length }}/{{ room.maxPlayers }}</span>
      </div>
      <WButton v-if="current" size="sm" variant="secondary" @click="$emit('open', room)">Abrir sala</WButton>
      <WButton
        v-else
        size="sm"
        :disabled="full"
        :loading="busy"
        @click="$emit('join', room)"
      >
        {{ full ? "Llena" : "Unirse" }}
      </WButton>
    </footer>
    <div class="room-card__meter" :style="{ '--fill': `${fill}%` }" aria-hidden="true" />
  </article>
</template>

<style scoped lang="scss">
.room-card {
  @include card(var(--s-5));
  @include interactive;
  position: relative;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  gap: var(--s-4);

  &:hover {
    border-color: var(--line-3);
    transform: translateY(-2px);
    box-shadow: var(--shadow-2);
  }

  &--current {
    border-color: var(--accent);
    box-shadow: var(--glow-accent);
  }

  &__head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--s-3);
  }
  &__title {
    min-width: 0;
    h3 {
      margin: 0;
      @include display-font(var(--fs-xl));
      @include truncate;
    }
  }
  &__host {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--fs-sm);
    color: var(--text-3);
    margin-top: 2px;
    svg {
      color: var(--in-room);
    }
  }
  &__tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  &__foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-3);
    margin-top: auto;
  }
  &__players {
    display: flex;
    align-items: center;
    gap: var(--s-3);
  }
  &__count {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--text-2);
  }
  &__meter {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 3px;
    width: var(--fill);
    background: var(--brand-gradient);
    opacity: 0.9;
  }
  &--ingame &__meter {
    background: var(--in-game);
  }
}
</style>
