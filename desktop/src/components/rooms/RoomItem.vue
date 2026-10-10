<script setup lang="ts">
import { LogIn, Trash2 } from "@lucide/vue";
import { computed, ref } from "vue";
import { useTicker } from "../../composables/useTicker";
import { useRoomsStore } from "../../store/rooms";
import { useServersStore } from "../../store/servers";
import { useSettingsStore } from "../../store/settings";
import type { RoomSummary } from "../../types";
import { formatSecondsAgo } from "../../utils/format";
import BaseButton from "../base/BaseButton.vue";
import ConfirmDialog from "../base/ConfirmDialog.vue";
import InlinePasswordPrompt from "../shared/InlinePasswordPrompt.vue";
import LockedBadge from "./LockedBadge.vue";
import RoomBadge from "./RoomBadge.vue";

const props = defineProps<{ room: RoomSummary; serverId: string }>();
const serversStore = useServersStore();
const roomsStore = useRoomsStore();
const settingsStore = useSettingsStore();
const nowSecs = useTicker();
const activeFor = computed(() => formatSecondsAgo(nowSecs.value - props.room.createdUnixSecs));

const connection = computed(() => serversStore.serverConnections.get(props.serverId));
const mine = computed(
  () => roomsStore.activeRoom?.room.id === props.room.id && roomsStore.activeRoom?.serverId === props.serverId,
);
const ownedByMe = computed(() => props.room.hostClientId === settingsStore.clientId);
const joinDisabled = computed(
  () => !connection.value?.authenticated || roomsStore.activeRoom !== null || props.room.status === "inProgress",
);
const pingText = computed(() => {
  const ms = connection.value?.pingMs;
  return ms !== null && ms !== undefined ? `${ms} ms` : "-- ms";
});

const showConfirmDelete = ref(false);

function join(password?: string): void {
  void roomsStore.joinRoom(props.serverId, props.room.id, password);
}

function confirmDelete(): void {
  showConfirmDelete.value = false;
  void roomsStore.deleteRoom(props.serverId, props.room.id);
}
</script>

<template>
  <div class="room-item" :class="{ 'room-item--mine': mine }" :data-testid="`room-item-${room.id}`">
    <div class="room-item__main">
      <div class="room-item__name">{{ room.name }}<span v-if="ownedByMe"> (tuya)</span></div>
      <div class="room-item__meta">
        <RoomBadge :status="room.status" />
        <LockedBadge v-if="room.hasPassword" />
        <span>Anfitrión: {{ room.hostDisplayName }}</span>
        <span>{{ room.playerCount }} jugador(es)</span>
        <span>Ping: {{ pingText }}</span>
        <span>Activa desde {{ activeFor }}</span>
      </div>
      <InlinePasswordPrompt
        v-if="!mine && room.hasPassword && !joinDisabled"
        placeholder="Contraseña de la sala"
        button-label="Unirse"
        :input-test-id="`room-password-input-${room.id}`"
        :button-test-id="`room-join-${room.id}`"
        :aria-label="`Contraseña para ${room.name}`"
        @submit="join"
      />
    </div>
    <div class="room-item__actions">
      <BaseButton
        v-if="!mine && !room.hasPassword"
        variant="tiny"
        :icon="LogIn"
        :icon-size="14"
        :disabled="joinDisabled"
        :test-id="`room-join-${room.id}`"
        :aria-label="`Unirse a ${room.name}`"
        @click="join()"
      >
        Unirse
      </BaseButton>
      <BaseButton
        v-if="ownedByMe && !mine"
        variant="tiny danger"
        :icon="Trash2"
        :icon-size="14"
        :test-id="`room-delete-${room.id}`"
        :aria-label="`Finalizar ${room.name}`"
        @click="showConfirmDelete = true"
      >
        Finalizar
      </BaseButton>
    </div>
    <ConfirmDialog
      v-if="showConfirmDelete"
      title="Finalizar sala"
      :message="`¿Finalizar '${room.name}' para todos? Esto no se puede deshacer.`"
      confirm-label="Finalizar"
      danger
      :test-id="`room-delete-confirm-${room.id}`"
      @confirm="confirmDelete"
      @cancel="showConfirmDelete = false"
    />
  </div>
</template>

<style scoped lang="scss">
.room-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: $spacing-md;
  padding: $spacing-md;
  background: rgba(255, 255, 255, 0.04);
  border: none;
  border-radius: $radius-lg;
  transition: $transition;

  &:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  // Same system purple as "your own" chat bubble -- a consistent "this
  // one is yours" marker distinct from the accent's "primary action"/
  // selection meaning.
  &--mine {
    background: $arcane-glow;
  }

  &__main {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  &__name {
    font-weight: 600;
    color: $text;
  }

  &__meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: $spacing-sm;
    font-size: $text-sm;
    color: $text-dim;
  }

  &__actions {
    display: flex;
    flex-shrink: 0;
    gap: $spacing-sm;
  }
}
</style>
