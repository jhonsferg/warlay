<script setup lang="ts">
import { PlugZap } from "@lucide/vue";
import { computed } from "vue";
import { useServersStore } from "../../store/servers";
import { useUiStore } from "../../store/ui";
import type { ServerInfo } from "../../types";
import { playHover, playClick } from "../../utils/audio";
import { connectionStatusTone } from "../../utils/status";
import BaseBadge from "../base/BaseBadge.vue";

const props = defineProps<{ server: ServerInfo }>();
const serversStore = useServersStore();
const uiStore = useUiStore();

const connection = computed(() => serversStore.serverConnections.get(props.server.id));
const active = computed(
  () => uiStore.railSelection?.kind === "server" && uiStore.railSelection.serverId === props.server.id,
);
// Password-required has its own retry flow already (InlinePasswordPrompt
// in ServerRoomsView) -- this button is specifically for the "the attempt
// itself failed" cases (a timeout, the hub rejecting/refusing the
// connection, or anything else that leaves the connection just sitting
// there disconnected with no other way to nudge it short of restarting
// the whole app).
const canRetry = computed(() => connection.value?.status === "disconnected" || connection.value?.status === "rejected");

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

function select(): void {
  playClick();
  uiStore.railSelection = { kind: "server", serverId: props.server.id };
}

function retryConnect(): void {
  playClick();
  void serversStore.connectServer(props.server.id);
}
</script>

<template>
  <div
    class="server-card"
    :class="{ 'server-card--active': active }"
    role="button"
    tabindex="0"
    :aria-current="active"
    :data-testid="`rail-server-${server.id}`"
    @mouseenter="playHover()"
    @click="select"
    @keydown.enter="select"
  >
    <span class="server-card__name">{{ server.name }}</span>
    <span class="server-card__address">{{ server.address }}</span>
    <span class="server-card__footer">
      <BaseBadge :tone="connectionStatusTone(connection?.status)">{{ statusLabel }}</BaseBadge>
      <span v-if="connection?.status === 'connected'" class="server-card__ping"
        >Ping: {{ connection.pingMs ?? "--" }} ms</span
      >
      <button
        v-if="canRetry"
        class="server-card__retry-btn"
        type="button"
        :aria-label="`Reintentar conexión a ${server.name}`"
        :title="`Reintentar conexión a ${server.name}`"
        :data-testid="`rail-server-retry-${server.id}`"
        @click.stop="retryConnect"
      >
        <PlugZap :size="12" />
      </button>
    </span>
  </div>
</template>

<style scoped lang="scss">
// A macOS source-list row: no border, no side accent stripe -- selection
// reads as a filled accent pill (same convention as RailIconButton, which
// sits in the same sidebar), same accent used by nav selection everywhere
// else in the shell.
.server-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
  width: 100%;
  padding: 10px 12px;
  background: transparent;
  border: none;
  border-radius: $radius-md;
  cursor: pointer;
  text-align: left;
  transition: $transition;
  color: $text;
  font-family: $font-body;

  &:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  &:focus-visible {
    outline: none;
    box-shadow: $focus-ring;
  }

  &--active {
    background: $gold-dim;

    .server-card__name {
      color: $gold-bright;
    }
  }

  &__name {
    font-weight: 600;
    color: $text;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__address {
    font-size: $text-xs;
    color: $text-muted;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;

    // Secondary info -- the first thing to go once the sidebar has
    // shrunk to $breakpoint-sm's narrower rail (see ServerRail.vue).
    @media (max-width: $breakpoint-sm) {
      display: none;
    }
  }

  &__footer {
    display: flex;
    align-items: center;
    gap: $spacing-sm;
    margin-top: 4px;
    flex-wrap: wrap;
  }

  &__ping {
    font-size: $text-xs;
    color: $text-dim;
  }

  // Only shown when the connection attempt itself failed (see canRetry) --
  // pushed to the far right of the footer row, away from the badge/ping.
  &__retry-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-left: auto;
    background: transparent;
    border: none;
    border-radius: 50%;
    padding: 3px;
    color: $text-dim;
    cursor: pointer;
    transition: $transition;

    &:hover {
      background: rgba(255, 255, 255, 0.12);
      color: $text;
    }

    &:active {
      transform: scale(0.92);
    }

    &:focus-visible {
      outline: none;
      box-shadow: $focus-ring;
    }
  }
}
</style>
