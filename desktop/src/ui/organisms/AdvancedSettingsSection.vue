<script setup lang="ts">
import AboutPanel from "./AboutPanel.vue";
import LogSettingsSection from "./LogSettingsSection.vue";
import UpdateSettingsSection from "./UpdateSettingsSection.vue";
</script>

<template>
  <div class="advanced" data-testid="advanced">
    <LogSettingsSection />
    <UpdateSettingsSection />
    <AboutPanel />
  </div>
</template>

<style scoped lang="scss">
.advanced {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
  gap: var(--s-5);
  align-items: start;
  max-width: 1100px;
  container-type: inline-size;
}
</style>
