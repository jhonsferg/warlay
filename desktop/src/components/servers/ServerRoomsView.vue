<script setup lang="ts">
import { Plus, Settings2 } from "@lucide/vue";
import { computed, ref } from "vue";
import { useServersStore } from "../../store/servers";
import { connectionStatusTone } from "../../utils/status";
import BaseBadge from "../base/BaseBadge.vue";
import BaseButton from "../base/BaseButton.vue";
import ChatCard from "../chat/ChatCard.vue";
import ActiveRoomPanel from "../rooms/ActiveRoomPanel.vue";
import CreateRoomModal from "../rooms/CreateRoomModal.vue";
import RoomList from "../rooms/RoomList.vue";
import InlinePasswordPrompt from "../shared/InlinePasswordPrompt.vue";
import ServerSettingsPanel from "./ServerSettingsPanel.vue";

const props = defineProps<{ serverId: string }>();
const serversStore = useServersStore();

const server = computed(() => serversStore.servers.get(props.serverId));
const connection = computed(() => serversStore.serverConnections.get(props.serverId));
const showSettings = ref(false);
const showCreateRoomModal = ref(false);

const statusLabel = computed(() => {
  switch (connection.value?.status) {
    case "connecting":
      return "Conectando…";
    case "connected":
      return "Conectado";
    case "passwordRequired":
      return "Requiere contraseña";
    case "rejected":
      return "Rechazado";
    default:
      return "Desconectado";
  }
});

function submitPassword(password: string): void {
  void serversStore.connectServer(props.serverId, password);
}
</script>

<template>
  <section v-if="server" class="server-rooms-view" data-testid="server-rooms-view">
    <header class="server-rooms-view__header">
      <div>
        <h2>{{ server.name }}</h2>
        <div class="server-rooms-view__meta">
          <BaseBadge :tone="connectionStatusTone(connection?.status)">{{ statusLabel }}</BaseBadge>
          <template v-if="connection?.status === 'connected'">
            <span>Ping: {{ connection.pingMs ?? "--" }} ms</span>
            <span>Usuarios: {{ connection.connectedClients ?? "--" }}</span>
          </template>
        </div>
      </div>
      <button
        class="server-rooms-view__settings-btn"
        type="button"
        aria-label="Ajustes del servidor"
        data-testid="server-settings-open"
        @click="showSettings = true"
      >
        <Settings2 :size="18" />
      </button>
    </header>

    <InlinePasswordPrompt
      v-if="connection?.status === 'passwordRequired'"
      placeholder="Contraseña del servidor"
      button-label="Conectar"
      input-test-id="server-password-input"
      button-test-id="server-password-submit"
      :aria-label="`Contraseña para ${server.name}`"
      @submit="submitPassword"
    />
    <p v-else-if="connection?.status === 'rejected'" class="empty">{{ connection.errorMessage }}</p>

    <div v-else class="server-rooms-view__body">
      <div class="server-rooms-view__left">
        <div class="card server-rooms-view__rooms-card">
          <div class="server-rooms-view__rooms-header">
            <h2>Salas activas</h2>
            <BaseButton
              variant="primary tiny"
              :icon="Plus"
              :icon-size="14"
              test-id="open-create-room"
              @click="showCreateRoomModal = true"
            >
              Crear sala
            </BaseButton>
          </div>
          <div class="server-rooms-view__rooms-scroll">
            <RoomList :server-id="serverId" />
          </div>
        </div>
        <ActiveRoomPanel :server-id="serverId" />
      </div>
      <ChatCard :server-id="serverId" />
    </div>
  </section>

  <CreateRoomModal v-if="showCreateRoomModal" :server-id="serverId" @close="showCreateRoomModal = false" />
  <ServerSettingsPanel v-if="showSettings && server" :server-id="serverId" @close="showSettings = false" />
</template>

<style scoped lang="scss">
.server-rooms-view {
  display: flex;
  flex-direction: column;
  gap: $spacing-lg;
  height: 100%;

  &__header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: $spacing-md;

    h2 {
      margin: 0 0 6px;
      border-bottom: none;
      padding-bottom: 0;
    }
  }

  &__meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: $spacing-sm;
    font-size: $text-sm;
    color: $text-dim;
  }

  &__settings-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    flex-shrink: 0;
    background: rgba(255, 255, 255, 0.08);
    border: none;
    border-radius: 50%;
    color: $text-dim;
    cursor: pointer;
    transition: $transition;

    &:hover {
      background: rgba(255, 255, 255, 0.16);
      color: $text;
    }

    &:active {
      transform: scale(0.92);
    }
  }

  &__body {
    display: grid;
    grid-template-columns: 1.3fr 1fr;
    // Without this, the implicit row has no height cap and auto-grows to
    // fit whichever column's content is tallest -- once the chat had
    // enough messages, that dragged the "Salas activas"/"Fundar sala"
    // column along with it and pushed the whole page into scroll instead
    // of letting ChatCard's own internal message list scroll. `minmax(0,
    // 1fr)` is the grid-row equivalent of `min-height: 0` on a flex item:
    // caps the row at the space actually available so each column clips
    // (and scrolls internally) on its own instead.
    grid-template-rows: minmax(0, 1fr);
    gap: $spacing-lg;
    min-height: 0;
    flex: 1;

    // A side-by-side split pane only works once there's real width to
    // split -- below $breakpoint-md it becomes one stacked column (rooms
    // above, chat below) that scrolls as a whole page instead of each
    // side scrolling independently. ChatCard's own internal scroll
    // (height: 100%) naturally falls back to just showing its full
    // content in this mode, since the grid row is no longer a fixed
    // height for it to fill.
    @media (max-width: $breakpoint-md) {
      grid-template-columns: 1fr;
      grid-template-rows: auto auto;
      overflow-y: auto;
      min-height: 0;
    }
  }

  &__left {
    display: flex;
    flex-direction: column;
    gap: $spacing-lg;
    min-width: 0;
    // No overflow of its own -- the rooms card below is the one that
    // scrolls internally now (see __rooms-scroll), so this column as a
    // whole stays exactly as tall as its grid cell (see __body's own
    // comment on minmax(0, 1fr)) rather than adding a second, competing
    // scroll container around it.
    min-height: 0;
  }

  // "Salas activas" flexes to fill whatever height __left has left over
  // after ActiveRoomPanel (when a room is active) -- its own header stays
  // put and only the room list underneath scrolls, instead of a growing
  // room list pushing the whole card (and everything below it) taller
  // than its container.
  &__rooms-card {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    gap: $spacing-md;
  }

  &__rooms-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: $spacing-md;
    border-bottom: 1px solid $border-bronze;
    padding-bottom: 0.5rem;

    h2 {
      margin: 0;
      border-bottom: none;
      padding-bottom: 0;
    }
  }

  &__rooms-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
}
</style>
