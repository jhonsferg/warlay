<script setup lang="ts">
import { Lock, LogOut, RotateCcw, Trash2, Unlock } from "@lucide/vue";
import { computed, ref } from "vue";
import { useRoomsStore } from "../../store/rooms";
import { useTicker } from "../../composables/useTicker";
import { formatSecondsAgo } from "../../utils/format";
import BaseButton from "../base/BaseButton.vue";
import ConfirmDialog from "../base/ConfirmDialog.vue";
import ActivityLog from "./ActivityLog.vue";
import RoomRoster from "./RoomRoster.vue";
import { playLeave } from "../../utils/audio";

const props = defineProps<{ serverId: string }>();
const roomsStore = useRoomsStore();
const nowSecs = useTicker();

const room = computed(() => (roomsStore.activeRoom?.serverId === props.serverId ? roomsStore.activeRoom.room : null));
const isHost = computed(() => roomsStore.activeRoom?.isHost ?? false);
const gameExited = computed(() => roomsStore.activeRoom?.gameExited ?? false);
const showConfirmDelete = ref(false);
const activeFor = computed(() => (room.value ? formatSecondsAgo(nowSecs.value - room.value.createdUnixSecs) : ""));

function resume(): void {
  void roomsStore.resumeGame();
}

function leave(): void {
  playLeave();
  void roomsStore.leaveRoom();
}

function confirmDelete(): void {
  showConfirmDelete.value = false;
  if (room.value) void roomsStore.deleteRoom(props.serverId, room.value.id);
}

/** Manual "cerrar sala"/"abrir sala" toggle, host-only — see
 * `closeRoomToNewJoiners`'s doc comment in the store for why this exists
 * alongside the automatic detection instead of replacing it. */
function toggleRoomLock(): void {
  if (!room.value) return;
  if (room.value.status === "inProgress") {
    void roomsStore.markRoomAvailable();
  } else {
    void roomsStore.closeRoomToNewJoiners();
  }
}
</script>

<template>
  <div v-if="room" class="active-room" data-testid="active-room">
    <div class="active-room__title">{{ isHost ? "Anfitrión de" : "Unido a" }}: {{ room.name }}</div>
    <div class="active-room__meta">
      Puerto de juego {{ room.gamePort }} · {{ room.playerCount }} jugador(es) ·
      {{ room.status === "inProgress" ? "En Partida" : "Sala abierta" }} · activa desde
      {{ activeFor }}
    </div>
    <p v-if="isHost" class="active-room__hint">
      Si abandonas la sala sin finalizarla, sigue disponible para el resto — solo "Finalizar sala" la cierra para todos.
    </p>
    <p v-if="isHost" class="active-room__hint">
      "Cerrar sala" no deja entrar a nadie más, como cerrar la puerta — los que ya están adentro siguen jugando normal,
      y podés volver a abrirla cuando quieras si falta alguien.
    </p>
    <p v-if="gameExited" class="active-room__hint">
      Saliste del juego, pero la sala sigue abierta — puedes reanudar cuando quieras.
    </p>
    <RoomRoster />
    <ActivityLog />
    <div class="active-room__actions">
      <BaseButton
        v-if="isHost"
        :variant="room.status === 'inProgress' ? 'primary tiny' : 'secondary tiny'"
        :icon="room.status === 'inProgress' ? Unlock : Lock"
        :icon-size="14"
        test-id="toggle-room-lock"
        @click="toggleRoomLock"
      >
        {{ room.status === "inProgress" ? "Abrir sala" : "Cerrar sala" }}
      </BaseButton>
      <BaseButton
        v-if="gameExited"
        variant="primary tiny"
        :icon="RotateCcw"
        :icon-size="14"
        test-id="resume-game"
        @click="resume"
      >
        Reanudar partida
      </BaseButton>
      <BaseButton variant="danger tiny" :icon="LogOut" :icon-size="14" test-id="leave-room" @click="leave">
        Abandonar sala
      </BaseButton>
      <BaseButton
        v-if="isHost"
        variant="danger tiny"
        :icon="Trash2"
        :icon-size="14"
        test-id="delete-room"
        @click="showConfirmDelete = true"
      >
        Finalizar sala
      </BaseButton>
    </div>
  </div>
  <ConfirmDialog
    v-if="showConfirmDelete && room"
    title="Finalizar sala"
    :message="`¿Finalizar '${room.name}' para todos? Esto no se puede deshacer — el resto de jugadores perderá la sala.`"
    confirm-label="Finalizar"
    danger
    test-id="delete-room-confirm"
    @confirm="confirmDelete"
    @cancel="showConfirmDelete = false"
  />
</template>

<style scoped lang="scss">
// Its own standalone card now (used to sit stacked below CreateRoomForm
// inside a shared card, hence the old margin-top/border-top separator --
// that form moved out to CreateRoomModal, so this needs its own panel
// treatment to stand on its own instead of leaving a stray top border
// with nothing above it).
.active-room {
  display: flex;
  flex-direction: column;
  gap: $spacing-sm;
  background: $bg-panel;
  border: 1px solid $border-bronze;
  border-radius: $radius-lg;
  box-shadow: $shadow-panel;
  padding: 20px;

  &__title {
    font-weight: 600;
    color: $text;
    font-size: $text-lg;
  }

  &__meta {
    font-size: $text-sm;
    color: $text-dim;
  }

  &__hint {
    margin: 0;
    font-size: $text-sm;
    color: $text-muted;
    font-style: italic;
  }

  &__actions {
    display: flex;
    flex-wrap: wrap;
    gap: $spacing-sm;
    margin-top: 4px;
  }
}
</style>
