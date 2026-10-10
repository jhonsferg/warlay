<script setup lang="ts">
import { Server } from "@lucide/vue";
import { ref } from "vue";
import BaseButton from "../base/BaseButton.vue";
import AddServerModal from "./AddServerModal.vue";

const showAddServerModal = ref(false);
</script>

<template>
  <div class="empty-state" data-testid="empty-state">
    <Server class="empty-state__icon" :size="40" />
    <h2>No tienes servidores registrados</h2>
    <p class="empty-state__hint">Registra un servidor para ver sus salas y empezar a jugar.</p>
    <BaseButton variant="primary" test-id="empty-state-add-server" @click="showAddServerModal = true">
      Registrar servidor
    </BaseButton>
  </div>
  <AddServerModal v-if="showAddServerModal" @close="showAddServerModal = false" />
</template>

<style scoped lang="scss">
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: $spacing-sm;
  height: 100%;
  text-align: center;
  padding: $spacing-lg;

  &__icon {
    color: $text-muted;
    margin-bottom: $spacing-sm;
  }

  &__hint {
    color: $text-dim;
    margin: 0 0 $spacing-md;
    max-width: 28rem;
  }

  h2 {
    margin: 0;
    color: $text-dim;
    border-bottom: none;
  }
}
</style>
