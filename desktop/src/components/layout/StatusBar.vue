<script setup lang="ts">
import { Play } from "@lucide/vue";
import { computed } from "vue";
import { useGameProcessStore } from "../../store/gameProcess";
import { useRoomsStore } from "../../store/rooms";
import { useServersStore } from "../../store/servers";
import { useSettingsStore } from "../../store/settings";
import BaseButton from "../base/BaseButton.vue";

const roomsStore = useRoomsStore();
const serversStore = useServersStore();
const settingsStore = useSettingsStore();
const gameProcessStore = useGameProcessStore();

const roomLabel = computed(() => {
  const room = roomsStore.activeRoom;
  if (!room) return "Sin sala activa";
  const serverName = serversStore.servers.get(room.serverId)?.name ?? room.serverId;
  return `${serverName} · ${room.room.name}`;
});

const hasGamePath = computed(() => settingsStore.settings.gameExecutablePath.trim().length > 0);

const buttonLabel = computed(() => {
  if (!hasGamePath.value) return "Sin ruta configurada";
  if (gameProcessStore.running) return "En ejecución";
  return "Iniciar Frozen Throne";
});

const buttonDisabled = computed(() => !hasGamePath.value || gameProcessStore.running);

function onLaunchClick(): void {
  void roomsStore.startGame();
}
</script>

<template>
  <div class="status-bar" data-testid="status-bar">
    <span class="status-bar__room" data-testid="status-bar-room">{{ roomLabel }}</span>
    <span
      class="status-bar__game"
      :class="{ 'status-bar__game--running': gameProcessStore.running }"
      :title="gameProcessStore.running ? 'Warcraft III en ejecución' : 'Warcraft III detenido'"
      data-testid="status-bar-game-state"
    >
      <span class="status-bar__dot" aria-hidden="true" />
      <span class="status-bar__game-text">{{
        gameProcessStore.running ? "Warcraft III en ejecución" : "Warcraft III detenido"
      }}</span>
    </span>
    <BaseButton
      variant="primary tiny"
      :icon="Play"
      :icon-size="14"
      :disabled="buttonDisabled"
      test-id="status-bar-launch"
      @click="onLaunchClick"
    >
      {{ buttonLabel }}
    </BaseButton>
  </div>
</template>

<style scoped lang="scss">
.status-bar {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 8px 16px;
  border-top: 1px solid $border-bronze;
  background: $bg-stone;
  backdrop-filter: $glass-blur;
  font-family: $font-body;
  font-size: $text-sm;
  color: $text-dim;

  &__room {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__game {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  // Below $breakpoint-sm, only the dot survives (title attribute on the
  // wrapping span carries the same text as a hover tooltip) -- keeps the
  // launch button from getting squeezed out on a narrow window.
  &__game-text {
    @media (max-width: $breakpoint-sm) {
      display: none;
    }
  }

  &__dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: $text-muted;
    flex-shrink: 0;
  }

  // Green reads as "live/running" the way a status dot does across macOS
  // (Messages online dot, menu-bar app "active" indicators) -- the accent
  // color is reserved for actions/selection, not this.
  &__game--running &__dot {
    background: $success;
  }
}
</style>
