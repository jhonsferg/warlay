<script setup lang="ts">
import { Lock } from "@lucide/vue";
import BaseBadge from "../base/BaseBadge.vue";
</script>

<template>
  <BaseBadge tone="steel">
    <Lock :size="10" />
    Con clave
  </BaseBadge>
</template>
