<script setup lang="ts">
import type { RoomStatus } from "../../types";
import BaseBadge from "../base/BaseBadge.vue";

defineProps<{ status: RoomStatus }>();
</script>

<template>
  <BaseBadge :tone="status === 'inProgress' ? 'amber' : 'success'">
    {{ status === "inProgress" ? "En Partida" : "Sala abierta" }}
  </BaseBadge>
</template>
