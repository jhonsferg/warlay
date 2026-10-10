<script setup lang="ts">
import BaseModal from "../base/BaseModal.vue";
import CreateRoomForm from "./CreateRoomForm.vue";

defineProps<{ serverId: string }>();
const emit = defineEmits<{ close: [] }>();
</script>

<template>
  <BaseModal title="Fundar una sala" test-id="create-room-modal" @close="emit('close')">
    <CreateRoomForm :server-id="serverId" @created="emit('close')" @cancel="emit('close')" />
  </BaseModal>
</template>
