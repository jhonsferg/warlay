<script setup lang="ts">
import BaseModal from "../base/BaseModal.vue";
import AddServerForm from "./AddServerForm.vue";

const emit = defineEmits<{ close: [] }>();
</script>

<template>
  <BaseModal title="Registrar servidor" test-id="add-server-modal" @close="emit('close')">
    <AddServerForm @added="emit('close')" />
  </BaseModal>
</template>
