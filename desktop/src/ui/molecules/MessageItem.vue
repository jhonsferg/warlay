<script setup lang="ts">
import { Check, CheckCheck, Lock, Trash2 } from "@lucide/vue";
import { computed, ref } from "vue";
import type { ChatMessage, MessageStatus } from "../../services/types";
import WAvatar from "../atoms/WAvatar.vue";

const props = defineProps<{
  message: ChatMessage;
  grouped: boolean;
  mine: boolean;
  /** Delivery state under our own direct messages; absent elsewhere. */
  status?: MessageStatus;
}>();
const emit = defineEmits<{ delete: [message: ChatMessage] }>();

const time = computed(() =>
  new Date(props.message.createdAt).toLocaleTimeString("es", { hour: "2-digit", minute: "2-digit" }),
);

const STATUS_LABEL: Record<MessageStatus, string> = { sent: "Enviado", delivered: "Recibido", read: "Leído" };

const confirming = ref(false);
function confirmDelete() {
  confirming.value = false;
  emit("delete", props.message);
}
</script>

<template>
  <div class="message" :class="{ 'message--grouped': grouped, 'message--mine': mine }" :data-message="message.body">
    <div class="message__gutter">
      <WAvatar v-if="!grouped" :name="message.sender.displayName" :color="message.sender.avatarColor" size="md" />
      <time v-else class="message__time-hover">{{ time }}</time>
    </div>
    <div class="message__body">
      <header v-if="!grouped" class="message__head">
        <span class="message__author" :style="{ color: message.sender.avatarColor }">{{
          message.sender.displayName
        }}</span>
        <time class="message__time">{{ time }}</time>
      </header>
      <p v-if="message.undecryptable" class="message__text message__text--locked" data-testid="undecryptable">
        <Lock :size="13" /> Mensaje cifrado que este dispositivo no puede leer
      </p>
      <p v-else class="message__text">{{ message.body }}</p>
      <span v-if="status || message.encrypted" class="message__meta">
        <Lock v-if="message.encrypted" :size="11" class="message__lock" title="Cifrado de extremo a extremo" />
        <span v-if="status" class="message__status" :class="`message__status--${status}`" :data-status="status">
          <CheckCheck v-if="status !== 'sent'" :size="13" />
          <Check v-else :size="13" />
          {{ STATUS_LABEL[status] }}
        </span>
      </span>
    </div>
    <div v-if="mine" class="message__actions">
      <template v-if="confirming">
        <button type="button" class="message__action message__action--danger" @click="confirmDelete">Borrar</button>
        <button type="button" class="message__action" @click="confirming = false">Cancelar</button>
      </template>
      <button
        v-else
        type="button"
        class="message__action"
        aria-label="Borrar mensaje"
        title="Borrar mensaje"
        @click="confirming = true"
      >
        <Trash2 :size="14" />
      </button>
    </div>
  </div>
</template>

<style scoped lang="scss">
.message {
  display: grid;
  grid-template-columns: 44px 1fr auto;
  gap: var(--s-3);
  padding: 6px var(--s-4);
  margin-top: var(--s-3);
  border-radius: var(--r-sm);

  // Proximity: consecutive messages from one sender read as one block.
  &--grouped {
    margin-top: 0;
    padding-top: 1px;
    padding-bottom: 1px;
  }

  &:hover {
    background: rgba(255, 255, 255, 0.025);
  }

  &__gutter {
    display: flex;
    justify-content: center;
  }

  &__time-hover {
    visibility: hidden;
    font-size: 10px;
    color: var(--text-3);
    line-height: 22px;
  }
  &:hover &__time-hover {
    visibility: visible;
  }

  &__head {
    display: flex;
    align-items: baseline;
    gap: var(--s-2);
  }
  &__author {
    font-weight: 700;
  }
  &__time {
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
  &__text {
    margin: 0;
    color: var(--text-1);
    white-space: pre-wrap;
    word-break: break-word;
    line-height: 1.5;
  }

  &__text--locked {
    display: flex;
    align-items: center;
    gap: 6px;
    font-style: italic;
    color: var(--text-3);
  }

  &__meta {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 2px;
  }
  &__lock {
    color: var(--text-3);
  }

  &__status {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--fs-xs);
    color: var(--text-3);

    &--read {
      color: var(--accent);
    }
  }

  // Shown on hover, like the grouped timestamp.
  &__actions {
    display: flex;
    align-items: flex-start;
    gap: var(--s-1);
    visibility: hidden;
  }
  &:hover &__actions,
  &:focus-within &__actions {
    visibility: visible;
  }
  &__action {
    all: unset;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    padding: 4px 6px;
    border-radius: var(--r-sm);
    font-size: var(--fs-xs);
    color: var(--text-3);

    &:hover,
    &:focus-visible {
      color: var(--text-1);
      background: rgba(255, 255, 255, 0.06);
    }
    &--danger {
      color: var(--danger, #f87171);
    }
  }
}
</style>
