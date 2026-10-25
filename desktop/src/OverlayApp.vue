<script setup lang="ts">
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { onMounted, onUnmounted, ref } from "vue";
import {
  getRoomMembers,
  onMatchmakingMessage,
  setOverlayInteractive,
} from "./api";
import type { RoomMember, ServerMessageEvent } from "./types";

const members = ref<RoomMember[]>([]);
const nowTick = ref<number>(Date.now());

let timer: ReturnType<typeof setInterval> | null = null;
const unlisteners: UnlistenFn[] = [];

onMounted(async () => {
  try {
    members.value = await getRoomMembers();
  } catch {
    members.value = [];
  }

  const unRoster = await listen<RoomMember[]>("overlay://roster", (event) => {
    members.value = event.payload;
  });
  unlisteners.push(unRoster);

  const unMsg = await onMatchmakingMessage((event: ServerMessageEvent) => {
    if (event.message.type === "roomMembersChanged") {
      members.value = event.message.members;
    } else if (event.message.type === "joinedRoom") {
      members.value = [];
    } else if (event.message.type === "leftRoom" || event.message.type === "roomRemoved") {
      members.value = [];
    }
  });
  unlisteners.push(unMsg);

  timer = setInterval(() => {
    nowTick.value = Date.now();
  }, 150);
});

onUnmounted(() => {
  if (timer) clearInterval(timer);
  for (const unlisten of unlisteners) {
    unlisten();
  }
});

function initial(name: string): string {
  return name.trim().charAt(0).toUpperCase() || "?";
}

function handleMouseEnter(): void {
  void setOverlayInteractive(true);
}

function handleMouseLeave(): void {
  void setOverlayInteractive(false);
}
</script>

<template>
  <div class="overlay-root">
    <div class="overlay-card" @mouseenter="handleMouseEnter" @mouseleave="handleMouseLeave">
      <div class="overlay-header">
        <span class="overlay-brand">WARLAY</span>
      </div>

      <div class="overlay-roster">
        <div v-if="members.length === 0" class="roster-empty">Sin jugadores en sala</div>
        <div v-else class="roster-grid">
          <div
            v-for="member in members"
            :key="member.clientId"
            class="peer-avatar"
            :title="member.displayName"
          >
            {{ initial(member.displayName) }}
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.overlay-root {
  width: 100vw;
  height: 100vh;
  margin: 0;
  padding: 8px;
  box-sizing: border-box;
  display: flex;
  align-items: flex-start;
  justify-content: flex-start;
  user-select: none;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
}

.overlay-card {
  width: 100%;
  max-width: 264px;
  background: rgba(32, 33, 36, 0.78);
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.overlay-header {
  display: flex;
  align-items: center;
}

.overlay-brand {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 1.5px;
  color: var(--text-dim);
  text-shadow: 0 1px 3px rgba(0, 0, 0, 0.9);
}

.overlay-controls {
  display: flex;
  align-items: center;
  gap: 10px;
}

.overlay-roster {
  min-height: 24px;
  display: flex;
  align-items: center;
}

.roster-empty {
  font-size: 11px;
  color: var(--text-dim);
  font-style: italic;
  text-shadow: 0 1px 3px rgba(0, 0, 0, 0.9);
}

.roster-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: center;
}

.peer-avatar {
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-dim);
  font-size: 11px;
  font-weight: 700;
  filter: drop-shadow(0 1px 3px rgba(0, 0, 0, 0.9));
  transition: color 0.2s ease, transform 0.2s ease;
}

.peer-avatar:hover {
  transform: scale(1.1);
}
</style>
