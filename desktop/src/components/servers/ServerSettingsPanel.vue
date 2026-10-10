<script setup lang="ts">
import { computed } from "vue";
import { useServersStore } from "../../store/servers";
import BaseModal from "../base/BaseModal.vue";
import DangerZone from "./DangerZone.vue";

const props = defineProps<{ serverId: string }>();
const emit = defineEmits<{ close: [] }>();
const serversStore = useServersStore();

const server = computed(() => serversStore.servers.get(props.serverId));
</script>

<template>
  <BaseModal v-if="server" :title="`Ajustes de ${server.name}`" test-id="server-settings-panel" @close="emit('close')">
    <div class="server-settings-panel__body">
      <div class="field">
        <span>Dirección</span>
        <div class="readonly-value">{{ server.address }}</div>
      </div>
      <div class="field">
        <span>Credenciales</span>
        <div class="readonly-value">
          {{ server.hasCredentials ? "Guardadas en este equipo" : "Sin guardar todavía" }}
        </div>
      </div>
    </div>
    <DangerZone :server-id="server.id" :server-name="server.name" @removed="emit('close')" />
  </BaseModal>
</template>

<style scoped lang="scss">
.server-settings-panel {
  &__body {
    display: flex;
    flex-direction: column;
    gap: $spacing-md;
    margin-bottom: $spacing-lg;
  }
}
</style>
