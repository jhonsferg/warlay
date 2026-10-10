<script setup lang="ts">
import { Mic, MicOff } from "@lucide/vue";
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useRoomsStore } from "../../store/rooms";
import { useSettingsStore } from "../../store/settings";
import type { RoomMember } from "../../types";

const roomsStore = useRoomsStore();
const settingsStore = useSettingsStore();

const members = computed<RoomMember[]>(() => roomsStore.activeRoom?.members ?? []);

const nowTick = ref<number>(Date.now());
let timer: ReturnType<typeof setInterval> | null = null;

onMounted(() => {
  timer = setInterval(() => {
    nowTick.value = Date.now();
  }, 150);
});

onUnmounted(() => {
  if (timer) clearInterval(timer);
});

function initial(name: string): string {
  return name.trim().charAt(0).toUpperCase() || "?";
}

function isSpeaking(clientId: string): boolean {
  // Read nowTick to force reactive updates on interval tick
  void nowTick.value;
  return roomsStore.isPeerSpeaking(clientId);
}

function isMuted(clientId: string): boolean {
  return roomsStore.isPeerMuted(clientId);
}

function toggleMute(clientId: string): void {
  roomsStore.toggleMutePeer(clientId);
}
</script>

<template>
  <div class="room-roster" data-testid="room-roster">
    <div class="room-roster__header">
      <span class="room-roster__title">Miembros de la sala</span>
      <span class="room-roster__count">({{ members.length }})</span>
    </div>

    <div v-if="members.length === 0" class="room-roster__empty">Cargando lista de miembros…</div>

    <div v-else class="room-roster__list">
      <div
        v-for="member in members"
        :key="member.clientId"
        class="room-roster__row"
        :class="{
          'room-roster__row--speaking': isSpeaking(member.clientId),
          'room-roster__row--muted': isMuted(member.clientId),
          'room-roster__row--me': member.clientId === settingsStore.clientId,
        }"
      >
        <div
          class="roster__avatar"
          :class="{ 'roster__avatar--speaking': isSpeaking(member.clientId) }"
          aria-hidden="true"
        >
          {{ initial(member.displayName) }}
        </div>

        <div class="room-roster__info">
          <span class="room-roster__name">
            {{ member.displayName }}
            <span v-if="member.clientId === settingsStore.clientId" class="room-roster__tag">(Tú)</span>
          </span>
          <span class="room-roster__status">
            <template v-if="isMuted(member.clientId)">Silenciado</template>
            <template v-else-if="isSpeaking(member.clientId)">Hablando…</template>
            <template v-else>En línea</template>
          </span>
        </div>

        <div class="room-roster__actions" v-if="member.clientId !== settingsStore.clientId">
          <button
            class="room-roster__mute-btn"
            :class="{ 'room-roster__mute-btn--muted': isMuted(member.clientId) }"
            :title="isMuted(member.clientId) ? 'Desactivar silencio' : 'Silenciar'"
            :aria-label="isMuted(member.clientId) ? 'Desactivar silencio' : 'Silenciar'"
            @click="toggleMute(member.clientId)"
          >
            <MicOff v-if="isMuted(member.clientId)" :size="16" />
            <Mic v-else :size="16" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.room-roster {
  display: flex;
  flex-direction: column;
  gap: $spacing-sm;
  margin-top: $spacing-sm;

  // Small-caps gray section label -- the macOS sidebar/list-section-header
  // idiom (e.g. Finder's "Favorites"), not an accented title.
  &__header {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: $text-xs;
    font-weight: 600;
    color: $text-muted;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  &__count {
    color: $text-muted;
    font-weight: normal;
  }

  &__empty {
    font-size: $text-sm;
    color: $text-muted;
    font-style: italic;
    padding: $spacing-sm 0;
  }

  &__list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  &__row {
    display: flex;
    align-items: center;
    gap: $spacing-sm;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: $radius-md;
    transition: $transition;

    &--speaking {
      background: $success-glow;
    }

    &--muted {
      opacity: 0.7;
    }
  }

  .roster__avatar {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background-color: rgba(255, 255, 255, 0.08);
    color: $text-dim;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 600;
    font-size: $text-sm;
    flex-shrink: 0;
    transition: $transition;

    &--speaking {
      color: $success;
      box-shadow: 0 0 8px $success-glow;
    }
  }

  &__info {
    display: flex;
    flex-direction: column;
    flex-grow: 1;
    min-width: 0;
  }

  &__name {
    font-size: $text-md;
    font-weight: 600;
    color: $text;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  &__tag {
    font-size: $text-xs;
    color: $gold-bright;
    margin-left: 4px;
    font-weight: normal;
  }

  &__status {
    font-size: $text-xs;
    color: $text-muted;
  }

  &__actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  // Same quiet circular icon-button language as the overlay's own mic
  // toggle -- transparent by default, a tinted fill only for the muted
  // state, no border.
  &__mute-btn {
    background: transparent;
    border: none;
    color: $text-dim;
    padding: 6px;
    border-radius: 50%;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: $transition;

    &:hover {
      background: rgba(255, 255, 255, 0.1);
      color: $text;
    }

    &:active {
      transform: scale(0.92);
    }

    &--muted {
      color: $danger;
      background: $danger-glow;

      &:hover {
        background: rgba(255, 69, 58, 0.25);
      }
    }
  }
}
</style>
