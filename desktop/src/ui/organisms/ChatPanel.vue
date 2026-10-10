<script setup lang="ts">
import { MessagesSquare, SendHorizontal } from "@lucide/vue";
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { errorMessage } from "../../services/http";
import type { ChannelKind } from "../../services/types";
import { channelKey, useChat } from "../../stores/chat";
import { useSession } from "../../stores/session";
import { useToasts } from "../../stores/toasts";
import WIconButton from "../atoms/WIconButton.vue";
import WSpinner from "../atoms/WSpinner.vue";
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
    return { message: m, grouped, mine: m.sender.id === session.me?.id };
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

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    void send();
  }
}

watch(key, open);
watch(() => messages.value.length, scrollToBottom);
onMounted(open);
onUnmounted(() => {
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
      />
    </div>
    <form class="chat-panel__composer" @submit.prevent="send">
      <textarea
        v-model="draft"
        class="chat-panel__input"
        rows="1"
        :placeholder="placeholder"
        maxlength="2000"
        aria-label="Escribe un mensaje"
        @keydown="onKeydown"
      />
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

  &__composer {
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
