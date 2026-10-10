<script setup lang="ts">
import { useUiStore } from "../../store/ui";
import BaseButton from "../base/BaseButton.vue";

const uiStore = useUiStore();
</script>

<template>
  <div
    class="banner"
    :class="{ 'banner--visible': uiStore.showExitBanner }"
    role="status"
    aria-live="polite"
    data-testid="exit-banner"
  >
    <span>Saliste del juego. Has sido desconectado de la sala.</span>
    <BaseButton
      variant="tiny secondary"
      test-id="exit-banner-dismiss"
      aria-label="Cerrar aviso de salida"
      @click="uiStore.showExitBanner = false"
      >Cerrar</BaseButton
    >
  </div>
</template>

<style scoped lang="scss">
// Never part of layout flow -- it has to slide in over whatever's
// underneath rather than pushing content down, and has to be able to hide
// completely (not just visually, height: 0 too) when showExitBanner is
// false, since this renders unconditionally (no v-if) and previously had no
// CSS at all governing that hidden state -- it showed permanently.
.banner {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: $spacing-md;
  padding: 0 $spacing-lg;
  max-height: 0;
  overflow: hidden;
  background: $bg-stone;
  backdrop-filter: $glass-blur;
  border-bottom: 1px solid transparent;
  color: $text;
  font-size: $text-sm;
  transition:
    max-height 0.25s ease,
    border-color 0.25s ease,
    padding 0.25s ease;

  // Amber, not gold -- this is a notice, not an invitation to click.
  &--visible {
    max-height: 56px;
    padding: 12px $spacing-lg;
    border-bottom-color: $amber;
  }
}
</style>
