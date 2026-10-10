<script setup lang="ts">
import { Mic, MicOff } from "@lucide/vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  getMutedPeers,
  getRoomMembers,
  isSelfMicMuted,
  loadSettings,
  onMatchmakingMessage,
  onMuteChanged,
  onSelfMuteChanged,
  onVoiceFrameIn,
  setOverlayInteractive,
  toggleSelfMicMute,
} from "./api";
import type { RoomMember, ServerMessageEvent } from "./types";

const selfMuted = ref<boolean>(true);
const members = ref<RoomMember[]>([]);
const speakingTimestamps = ref<Record<string, number>>({});
const mutedPeers = ref<Set<string>>(new Set());
const nowTick = ref<number>(Date.now());
// Read once at mount -- this is a separate Tauri window/JS realm from the
// main app, so it can't just read the settings store; a key changed
// mid-session (rare -- normally configured before playing) won't be
// reflected here until the overlay is recreated (app restart).
const micToggleKey = ref<string>("");

let timer: ReturnType<typeof setInterval> | null = null;
const unlisteners: UnlistenFn[] = [];

onMounted(async () => {
  try {
    selfMuted.value = await isSelfMicMuted();
  } catch {
    selfMuted.value = true;
  }

  try {
    const muted = await getMutedPeers();
    mutedPeers.value = new Set(muted);
  } catch {}

  // Same "ask for current truth on mount" pattern as the mic/mute state
  // above -- without this, the overlay (created only once the game
  // actually launches, typically well after the room already has members)
  // had nothing to show until a *new* roomMembersChanged push happened to
  // arrive, which could be never for the rest of the session.
  try {
    members.value = await getRoomMembers();
  } catch {
    members.value = [];
  }

  try {
    const settings = await loadSettings();
    micToggleKey.value = settings.micToggleKey;
  } catch {
    // Key badge just shows nothing if this fails -- not worth blocking
    // the rest of the overlay over.
  }

  const unSelfMute = await onSelfMuteChanged((muted) => {
    selfMuted.value = muted;
  });
  unlisteners.push(unSelfMute);

  // Room sessions (main server + helpers) push the roster directly.
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

  const unVoice = await onVoiceFrameIn(({ fromClientId }) => {
    speakingTimestamps.value[fromClientId] = Date.now();
  });
  unlisteners.push(unVoice);

  const unMute = await onMuteChanged((mutedList) => {
    mutedPeers.value = new Set(mutedList);
  });
  unlisteners.push(unMute);

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

async function handleToggleSelfMute(): Promise<void> {
  selfMuted.value = await toggleSelfMicMute();
}

const micTitle = computed(() => {
  const state = selfMuted.value ? "Micrófono silenciado" : "Micrófono activado";
  return micToggleKey.value ? `${state} — atajo: ${micToggleKey.value}` : state;
});

function initial(name: string): string {
  return name.trim().charAt(0).toUpperCase() || "?";
}

function isPeerSpeaking(clientId: string): boolean {
  void nowTick.value;
  const lastTime = speakingTimestamps.value[clientId] || 0;
  return nowTick.value - lastTime < 400;
}

function isPeerMuted(clientId: string): boolean {
  return mutedPeers.value.has(clientId);
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

      <div class="overlay-controls">
        <button
          class="control-btn control-btn--mic"
          :class="{ 'control-btn--muted': selfMuted }"
          :title="micTitle"
          @click="handleToggleSelfMute"
        >
          <MicOff v-if="selfMuted" :size="22" />
          <Mic v-else :size="22" />
          <span v-if="micToggleKey" class="control-btn__key">{{ micToggleKey }}</span>
        </button>
      </div>

      <div class="overlay-roster">
        <div v-if="members.length === 0" class="roster-empty">Sin jugadores en sala</div>
        <div v-else class="roster-grid">
          <div
            v-for="member in members"
            :key="member.clientId"
            class="peer-avatar"
            :class="{
              'peer-avatar--speaking': isPeerSpeaking(member.clientId),
              'peer-avatar--muted': isPeerMuted(member.clientId),
            }"
            :title="`${member.displayName}${isPeerMuted(member.clientId) ? ' (Silenciado)' : isPeerSpeaking(member.clientId) ? ' (Hablando)' : ''}`"
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
  background: transparent;
  display: flex;
  align-items: flex-start;
  justify-content: flex-start;
  user-select: none;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
}

// No frame, no fill anywhere below -- every element sits directly on the
// game's own video, so legibility comes from `drop-shadow`/`text-shadow`
// halos instead of a background plate. $text-dim (from the shared design
// tokens -- this file now consumes the same tokens as the rest of the app instead
// of hand-picked hex) is the neutral resting color for every control; color
// only ever signals state (muted, speaking), never decoration.
.overlay-card {
  width: 100%;
  max-width: 264px;
  background: transparent;
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
  color: $text-dim;
  text-shadow: 0 1px 3px rgba(0, 0, 0, 0.9);
}

// Google Meet's own bottom control bar: big filled circles, icon only,
// color is the only thing that ever signals state (muted = red fill,
// active = a green ring) -- no text labels, since a Meet-style bar reads
// entirely through shape/color at a glance instead of being read.
.overlay-controls {
  display: flex;
  align-items: center;
  gap: 10px;
}

.control-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 48px;
  border-radius: 50%;
  background: rgba(32, 33, 36, 0.78);
  border: none;
  color: $text;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.45);
  transition:
    background-color 0.15s ease,
    box-shadow 0.15s ease,
    transform 0.1s ease;

  &__key {
    position: absolute;
    bottom: -4px;
    right: -4px;
    padding: 1px 5px;
    border-radius: $radius-sm;
    background: rgba(20, 20, 22, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.16);
    color: $text-dim;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.2px;
  }
}

// Mic toggle: the actual click target for muting/unmuting.
.control-btn--mic {
  cursor: pointer;

  &:hover {
    background: rgba(48, 49, 52, 0.88);
  }

  &:active {
    transform: scale(0.94);
  }
}

// Meet's own "muted" red -- filled, not just a tinted icon, so it reads
// unmistakably even at a glance from across the game window.
.control-btn--mic.control-btn--muted {
  background: $danger;

  &:hover {
    background: darken($danger, 6%);
  }
}

.overlay-roster {
  min-height: 24px;
  display: flex;
  align-items: center;
}

.roster-empty {
  font-size: 11px;
  color: $text-dim;
  font-style: italic;
  text-shadow: 0 1px 3px rgba(0, 0, 0, 0.85);
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
  color: $text-dim;
  font-size: 11px;
  font-weight: 700;
  filter: drop-shadow(0 1px 3px rgba(0, 0, 0, 0.9));
  transition:
    color 0.2s ease,
    transform 0.2s ease;
}

.peer-avatar--speaking {
  color: $success;
  transform: scale(1.18);
  filter: drop-shadow(0 0 6px rgba($success, 0.85)) drop-shadow(0 1px 2px rgba(0, 0, 0, 0.9));
}

.peer-avatar--muted {
  color: $text-muted;
  opacity: 0.6;
}
</style>
