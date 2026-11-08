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
  display: flex;
  flex-wrap: wrap;
  gap: var(--s-5);
  align-items: stretch;
  max-width: 1100px;
  margin: 0 auto;
  container-type: inline-size;

  // Each block grows to share the row, so nothing is left empty.
  > * {
    flex: 1 1 340px;
    min-width: 0;
  }
  // The about panel always takes a full row of its own.
  > :last-child {
    flex-basis: 100%;
  }
}
</style>
