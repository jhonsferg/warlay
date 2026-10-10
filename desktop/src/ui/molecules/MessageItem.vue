<script setup lang="ts">
import { computed } from "vue";
import type { ChatMessage } from "../../services/types";
import WAvatar from "../atoms/WAvatar.vue";

const props = defineProps<{ message: ChatMessage; grouped: boolean; mine: boolean }>();

const time = computed(() =>
  new Date(props.message.createdAt).toLocaleTimeString("es", { hour: "2-digit", minute: "2-digit" }),
);
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
      <p class="message__text">{{ message.body }}</p>
    </div>
  </div>
</template>

<style scoped lang="scss">
.message {
  display: grid;
  grid-template-columns: 44px 1fr;
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
}
</style>
