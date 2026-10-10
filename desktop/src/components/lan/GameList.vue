<script setup lang="ts">
import { useLanMeshStore } from "../../store/lanMesh";
import GameItem from "./GameItem.vue";

const lanMeshStore = useLanMeshStore();
</script>

<template>
  <div class="game-list" data-testid="game-list">
    <p v-if="!lanMeshStore.selectedRelayAddr" class="empty">Selecciona un servidor de relay para ver sus partidas.</p>
    <p v-else-if="lanMeshStore.games.length === 0" class="empty">Sin partidas observadas en este relay todavía.</p>
    <GameItem v-for="game in lanMeshStore.games" :key="game.sourceAddr" :game="game" />
  </div>
</template>

<style scoped lang="scss">
.game-list {
  display: flex;
  flex-direction: column;
  gap: $spacing-sm;
}
</style>
