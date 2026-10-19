<script setup lang="ts">
import { MessagesSquare, SendHorizontal, Smile } from "@lucide/vue";
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { errorMessage } from "../../services/http";
import type { ChannelKind, ChatMessage } from "../../services/types";
import { channelKey, useChat } from "../../stores/chat";
import { useSession } from "../../stores/session";
import { useToasts } from "../../stores/toasts";
import WIconButton from "../atoms/WIconButton.vue";
import WSpinner from "../atoms/WSpinner.vue";
import EmojiPicker from "../molecules/EmojiPicker.vue";
import EmptyState from "../molecules/EmptyState.vue";
import MessageItem from "../molecules/MessageItem.vue";

const props = defineProps<{ kind: ChannelKind; targetId: string; placeholder: string; emptyTitle?: string }>();

const chat = useChat();
const session = useSession();
const toasts = useToasts();
const draft = ref("");
const sending = ref(false);
const loading = ref(true);
const scroller = ref<HTMLElement | null>(null);
const composer = ref<HTMLElement | null>(null);
const input = ref<HTMLTextAreaElement | null>(null);
const emojiOpen = ref(false);

const key = computed(() => channelKey(props.kind, props.targetId));
const messages = computed(() => chat.channel(key.value).messages);

// Group consecutive messages by the same sender within 5 minutes.
const rows = computed(() =>
  messages.value.map((m, i) => {
    const prev = messages.value[i - 1];
    const grouped =
      !!prev &&
      prev.sender.id === m.sender.id &&
      new Date(m.createdAt).getTime() - new Date(prev.createdAt).getTime() < 5 * 60_000;
    const mine = m.sender.id === session.me?.id;
    // Delivery state is a direct-message thing; groups have no single reader.
    return { message: m, grouped, mine, status: mine && props.kind === "dm" ? chat.statusOf(m) : undefined };
  }),
);

async function scrollToBottom() {
  await nextTick();
  if (scroller.value) scroller.value.scrollTop = scroller.value.scrollHeight;
}

async function open() {
  loading.value = true;
  chat.focus(key.value);
  try {
    await chat.load(props.kind, props.targetId);
  } catch (error) {
    toasts.error("No se pudo cargar el chat", errorMessage(error));
  } finally {
    loading.value = false;
    void scrollToBottom();
  }
}

async function send() {
  const body = draft.value.trim();
  if (!body || sending.value) return;
  sending.value = true;
  try {
    await chat.send(props.kind, props.targetId, body);
    draft.value = "";
    void scrollToBottom();
  } catch (error) {
    toasts.error("No se pudo enviar el mensaje", errorMessage(error));
  } finally {
    sending.value = false;
  }
}

async function remove(message: ChatMessage) {
  try {
    await chat.remove(message);
  } catch (error) {
    toasts.error("No se pudo borrar el mensaje", errorMessage(error));
  }
}

/** Inserts at the caret, so an emoji lands where the user was typing. */
async function insertEmoji(emoji: string) {
  const el = input.value;
  const start = el?.selectionStart ?? draft.value.length;
  const end = el?.selectionEnd ?? draft.value.length;
  draft.value = draft.value.slice(0, start) + emoji + draft.value.slice(end);
  await nextTick();
  el?.focus();
  el?.setSelectionRange(start + emoji.length, start + emoji.length);
}

function closeEmojiOnOutsideClick(e: MouseEvent) {
  if (emojiOpen.value && composer.value && !composer.value.contains(e.target as Node)) emojiOpen.value = false;
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") emojiOpen.value = false;
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    void send();
  }
}

watch(key, open);
watch(() => messages.value.length, scrollToBottom);
onMounted(() => {
  document.addEventListener("mousedown", closeEmojiOnOutsideClick);
  void open();
});
onUnmounted(() => {
  document.removeEventListener("mousedown", closeEmojiOnOutsideClick);
  if (chat.focused === key.value) chat.focus(null);
});
</script>

<template>
  <section class="chat-panel">
    <div ref="scroller" class="chat-panel__scroll" data-testid="chat-messages">
      <div v-if="loading" class="chat-panel__loading"><WSpinner :size="22" /></div>
      <EmptyState
        v-else-if="messages.length === 0"
        :title="emptyTitle ?? 'Aún no hay mensajes'"
        text="Rompe el hielo: el primer mensaje es tuyo."
      >
        <template #icon><MessagesSquare /></template>
      </EmptyState>
      <MessageItem
        v-for="row in rows"
        :key="row.message.id"
        :message="row.message"
        :grouped="row.grouped"
        :mine="row.mine"
        :status="row.status"
        @delete="remove"
      />
    </div>
    <form ref="composer" class="chat-panel__composer" @submit.prevent="send">
      <div v-if="emojiOpen" class="chat-panel__emoji"><EmojiPicker @pick="insertEmoji" /></div>
      <textarea
        ref="input"
        v-model="draft"
        class="chat-panel__input"
        rows="1"
        :placeholder="placeholder"
        maxlength="2000"
        aria-label="Escribe un mensaje"
        @keydown="onKeydown"
      />
      <WIconButton label="Emojis" :active="emojiOpen" @click="emojiOpen = !emojiOpen"><Smile /></WIconButton>
      <WIconButton label="Enviar" :active="draft.trim().length > 0" @click="send"><SendHorizontal /></WIconButton>
    </form>
  </section>
</template>

<style scoped lang="scss">
.chat-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  height: 100%;

  &__scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--s-2) 0 var(--s-4);
  }
  &__loading {
    display: grid;
    place-items: center;
    padding: var(--s-10);
    color: var(--text-3);
  }

  &__emoji {
    position: absolute;
    right: 0;
    bottom: calc(100% + var(--s-2));
    z-index: 10;
  }

  &__composer {
    position: relative;
    display: flex;
    align-items: flex-end;
    gap: var(--s-2);
    margin: 0 var(--s-4) var(--s-4);
    padding: var(--s-2) var(--s-2) var(--s-2) var(--s-4);
    background: var(--bg-4);
    border: 1px solid var(--line-2);
    border-radius: var(--r-md);
    transition: border-color var(--t-fast);

    &:focus-within {
      border-color: var(--accent);
    }
  }
  &__input {
    flex: 1;
    resize: none;
    border: 0;
    outline: 0;
    background: transparent;
    padding: 8px 0;
    max-height: 140px;
    line-height: 1.4;
    &::placeholder {
      color: var(--text-3);
    }
  }
}
</style>
