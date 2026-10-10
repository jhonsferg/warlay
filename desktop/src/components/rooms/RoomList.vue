<script setup lang="ts">
import { computed } from "vue";
import { useServersStore } from "../../store/servers";
import RoomItem from "./RoomItem.vue";

const props = defineProps<{ serverId: string }>();
const serversStore = useServersStore();

const connection = computed(() => serversStore.serverConnections.get(props.serverId));
const rooms = computed(() => (connection.value ? [...connection.value.rooms.values()] : []));
</script>

<template>
  <div class="room-list" data-testid="room-list">
    <p v-if="rooms.length === 0" class="empty">No hay salas activas. ¡Funda la primera!</p>
    <RoomItem v-for="room in rooms" :key="room.id" :room="room" :server-id="serverId" />
  </div>
</template>

<style scoped lang="scss">
.room-list {
  display: flex;
  flex-direction: column;
  gap: $spacing-sm;
}
</style>
