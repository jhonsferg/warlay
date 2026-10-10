<script setup lang="ts">
import { Check, Copy } from "@lucide/vue";
import { computed, ref } from "vue";
import type { GameInfo } from "../../types";
import { formatSecondsAgo } from "../../utils/format";
import BaseButton from "../base/BaseButton.vue";

const props = defineProps<{ game: GameInfo }>();

const copied = ref(false);

async function copyIp(): Promise<void> {
  const ip = props.game.sourceAddr.split(":")[0];
  await navigator.clipboard.writeText(ip);
  copied.value = true;
  setTimeout(() => (copied.value = false), 1200);
}

const copyIcon = computed(() => (copied.value ? Check : Copy));
</script>

<template>
  <div class="game-item" :data-testid="`game-item-${game.sourceAddr}`">
    <div>
      <div class="game-item__name">{{ game.gameName ?? "Partida sin nombre detectable" }}</div>
      <div class="game-item__meta">
        {{ game.sourceAddr }} · {{ formatSecondsAgo(game.lastSeenSecsAgo) }} · {{ game.payloadLen }} bytes
      </div>
    </div>
    <BaseButton
      variant="tiny"
      :icon="copyIcon"
      :icon-size="14"
      :test-id="`game-copy-ip-${game.sourceAddr}`"
      :aria-label="`Copiar IP de ${game.sourceAddr}`"
      @click="copyIp"
      >{{ copied ? "Copiada" : "Copiar IP" }}</BaseButton
    >
  </div>
</template>

<style scoped lang="scss">
.game-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: $spacing-md;
  padding: $spacing-sm $spacing-md;
  background: rgba(255, 255, 255, 0.04);
  border: none;
  border-radius: $radius-md;

  &__name {
    color: $text;
    font-size: $text-md;
  }

  &__meta {
    font-size: $text-xs;
    color: $text-muted;
  }
}
</style>
