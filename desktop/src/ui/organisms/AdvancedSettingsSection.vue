<script setup lang="ts">
import { Server } from "@lucide/vue";
import { mainServerUrl } from "../../services/http";
import WInput from "../atoms/WInput.vue";
import FormField from "../molecules/FormField.vue";
import AboutPanel from "./AboutPanel.vue";
import LogSettingsSection from "./LogSettingsSection.vue";
import UpdateSettingsSection from "./UpdateSettingsSection.vue";

const serverUrl = mainServerUrl();
</script>

<template>
  <div class="advanced" data-testid="advanced">
    <section class="advanced__card">
      <h3><Server /> Servidor de Warlay</h3>
      <FormField
        label="Dirección del servidor principal"
        for="server-url"
        hint="Solo lectura: la define la instalación."
      >
        <WInput id="server-url" :model-value="serverUrl" readonly data-testid="server-url" />
      </FormField>
    </section>

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

  &__card {
    @include card(var(--s-5));
    display: flex;
    flex-direction: column;
    gap: var(--s-4);

    h3 {
      display: flex;
      align-items: center;
      gap: var(--s-2);
      margin: 0;
    }
  }
}
</style>
