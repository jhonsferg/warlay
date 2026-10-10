<script setup lang="ts">
import { toTypedSchema } from "@vee-validate/zod";
import { Send, UserPlus, X } from "@lucide/vue";
import { computed, nextTick, ref, watch } from "vue";
import { useField } from "vee-validate";
import { useServersStore } from "../../store/servers";
import { useSettingsStore } from "../../store/settings";
import { useChatStore } from "../../store/chat";
import { chatMessageSchema } from "../../utils/validation";
import BaseButton from "../base/BaseButton.vue";
import BaseInput from "../base/BaseInput.vue";
import HintText from "../base/HintText.vue";

const props = defineProps<{ serverId: string }>();
const serversStore = useServersStore();
const settingsStore = useSettingsStore();
const chatStore = useChatStore();

const connection = computed(() => serversStore.serverConnections.get(props.serverId));

const activeTab = ref<string>("global"); // "global" or clientId

const messages = computed(() => {
  if (activeTab.value === "global") {
    return connection.value?.chatMessages ?? [];
  } else {
    return chatStore.privateMessages.get(activeTab.value) ?? [];
  }
});

const {
  value: draft,
  errorMessage,
  validate,
  resetField,
} = useField<string>("chatMessage", toTypedSchema(chatMessageSchema), {
  initialValue: "",
});
const listEl = ref<HTMLDivElement | null>(null);

watch(
  () => messages.value.length,
  async () => {
    await nextTick();
    if (listEl.value) listEl.value.scrollTop = listEl.value.scrollHeight;
  },
);
watch(activeTab, async () => {
  await nextTick();
  if (listEl.value) listEl.value.scrollTop = listEl.value.scrollHeight;
});

function initial(name: string): string {
  return name.trim().charAt(0).toUpperCase() || "?";
}

function formatTime(unixMillis: number): string {
  return new Date(unixMillis).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

async function send(): Promise<void> {
  const result = await validate();
  if (!result.valid || !result.value) return;

  const text = result.value;
  resetField({ value: "" });

  if (activeTab.value !== "global") {
    void serversStore.sendPrivateMessage(props.serverId, activeTab.value, text);
  } else {
    void serversStore.sendChatMessage(props.serverId, text);
  }
}

function switchTab(tab: string) {
  activeTab.value = tab;
}

function addFriend(clientId: string, displayName: string) {
  if (clientId === settingsStore.clientId) return;
  chatStore.addFriend(clientId, displayName);
  activeTab.value = clientId;
}

function removeFriend(clientId: string, event: Event) {
  event.stopPropagation();
  chatStore.removeFriend(clientId);
  if (activeTab.value === clientId) {
    activeTab.value = "global";
  }
}
</script>

<template>
  <div class="card chat__card">
    <div class="chat__tabs">
      <div class="chat__tab" :class="{ 'chat__tab--active': activeTab === 'global' }" @click="switchTab('global')">
        Global
      </div>
      <div
        v-for="[id, friend] in chatStore.friends"
        :key="id"
        class="chat__tab"
        :class="{ 'chat__tab--active': activeTab === id }"
        @click="switchTab(id)"
      >
        {{ friend.displayName }}
        <button class="chat__tab-close" @click="removeFriend(id, $event)" aria-label="Cerrar chat">
          <X :size="12" />
        </button>
      </div>
    </div>

    <div ref="listEl" class="chat__messages" role="log" aria-live="polite" data-testid="chat-messages">
      <p v-if="messages.length === 0" class="empty">Nadie ha dicho nada todavía.</p>
      <div
        v-for="(entry, index) in messages"
        :key="index"
        class="chat__row"
        :class="{ 'chat__row--mine': entry.fromClientId === settingsStore.clientId }"
      >
        <div class="chat__avatar" aria-hidden="true">{{ initial(entry.fromDisplayName) }}</div>
        <div class="chat__bubble">
          <div class="chat__bubble-header">
            <span class="chat__message-author">
              {{ entry.fromDisplayName }}
              <button
                v-if="entry.fromClientId !== settingsStore.clientId && !chatStore.friends.has(entry.fromClientId)"
                class="chat__add-friend-btn"
                title="Añadir amigo"
                @click="addFriend(entry.fromClientId, entry.fromDisplayName)"
              >
                <UserPlus :size="12" />
              </button>
            </span>
            <span class="chat__message-time">{{ formatTime(entry.unixMillis) }}</span>
          </div>
          <div class="chat__message-text">{{ entry.text }}</div>
        </div>
      </div>
    </div>
    <div class="input-group">
      <BaseInput
        v-model="draft"
        placeholder="Escribe un mensaje…"
        test-id="chat-input"
        aria-label="Mensaje de chat"
        @keydown.enter="send"
      />
      <BaseButton
        variant="primary"
        :icon="Send"
        :icon-size="14"
        test-id="chat-send"
        aria-label="Enviar mensaje"
        @click="send"
      />
    </div>
    <HintText :text="errorMessage" :error="!!errorMessage" test-id="chat-hint" />
  </div>
</template>

<style scoped lang="scss">
.chat__card {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.chat__tabs {
  display: flex;
  overflow-x: auto;
  gap: 8px;
  margin-bottom: 12px;
  border-bottom: 1px solid $border-bronze;
  padding-bottom: 8px;
}
.chat__tab {
  padding: 6px 12px;
  background: rgba(255, 255, 255, 0.05);
  border: none;
  border-radius: $radius-md;
  cursor: pointer;
  font-size: $text-sm;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 6px;
  transition: $transition;
  white-space: nowrap;

  &:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  // Same accent-tinted selection as sidebar nav/rooms -- switching tabs is
  // navigation, so it shares that convention instead of a separate steel
  // "you're here" color.
  &--active {
    background: $gold-dim;
    color: $gold-bright;
  }
}
.chat__tab-close {
  background: none;
  border: none;
  color: $text-muted;
  cursor: pointer;
  padding: 2px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  transition: $transition;

  &:hover {
    color: $danger;
    background: $danger-glow;
  }

  &:active {
    transform: scale(0.92);
  }
}
.chat__add-friend-btn {
  background: none;
  border: none;
  color: $text-muted;
  cursor: pointer;
  margin-left: 4px;
  padding: 2px;
  border-radius: 50%;
  transition: $transition;

  &:hover {
    color: $gold-bright;
    background: $gold-dim;
  }

  &:active {
    transform: scale(0.92);
  }
}

.chat__messages {
  flex-grow: 1;
  overflow-y: auto;
  padding: var(--spacing-sm);
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
  border-radius: var(--radius-md);
  background-color: rgba(0, 0, 0, 0.2);
  margin-bottom: var(--spacing-md);
}

.chat__row {
  display: flex;
  gap: var(--spacing-sm);
  align-items: flex-end;
  max-width: 90%;

  // Your own bubble gets its own hue (system purple) -- not the accent
  // used for the primary button/badges/selection, just something that
  // reads as "this one's mine" at a glance.
  &--mine {
    align-self: flex-end;
    flex-direction: row-reverse;

    .chat__bubble {
      border-bottom-left-radius: var(--radius-lg);
      border-bottom-right-radius: 2px;
      background-color: $arcane-glow;
    }
  }
}

.chat__avatar {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background-color: rgba(255, 255, 255, 0.08);
  color: $text-dim;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 600;
  font-size: $text-sm;
  flex-shrink: 0;
}

.chat__bubble {
  background-color: rgba(255, 255, 255, 0.06);
  padding: var(--spacing-sm) var(--spacing-md);
  border-radius: var(--radius-lg);
  border-bottom-left-radius: 2px;
  box-shadow: $shadow-panel;
  display: flex;
  flex-direction: column;
  gap: 2px;

  &-header {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-md);
    font-size: $text-xs;
    margin-bottom: 4px;
  }
}

// Neutral, not accented -- a sender name isn't an action, and coloring
// every single message author the same as a primary button would make
// the accent stop meaning anything.
.chat__message-author {
  font-weight: 600;
  color: $text;
  display: inline-flex;
  align-items: center;
}

.chat__message-time {
  color: $text-muted;
}

.chat__message-text {
  font-size: $text-md;
  color: $text;
  word-break: break-word;
}
</style>
