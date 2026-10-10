<script setup lang="ts">
import BaseModal from "../base/BaseModal.vue";

defineProps<{
  open: boolean;
}>();

const emit = defineEmits<{
  (e: "close"): void;
}>();
</script>

<template>
  <BaseModal v-if="open" title="Acerca de Warlay" @close="emit('close')">
    <div class="about-modal__body">
      <p class="about-modal__description">
        Warlay es un cliente avanzado de red y emparejamiento para emular redes LAN locales en partidas multijugador de
        Warcraft III, reduciendo drásticamente la latencia gracias a nuestro demonio de red y puente IPC.
      </p>

      <h3 class="about-modal__subtitle">Licencias y Créditos</h3>
      <ul class="about-modal__list">
        <li>
          <strong>Sonido de Notificación:</strong> "Beep-09" por <em>Rockhusho</em> de Wikimedia Commons (CC BY-SA 4.0)
        </li>
        <li><strong>Framework UI:</strong> Vue.js 3 y Vite (MIT)</li>
        <li><strong>Iconos:</strong> Lucide Icons (ISC)</li>
        <li><strong>Backend y Empaquetado:</strong> Tauri 2 (MIT / Apache-2.0) y Rust (MIT)</li>
        <li><strong>Motor de Red:</strong> Warlay Engine (MIT - Propietario)</li>
      </ul>

      <div class="about-modal__footer">
        <p>© 2026 Warlay. Todos los derechos reservados.</p>
      </div>
    </div>
  </BaseModal>
</template>

<style scoped lang="scss">
.about-modal__body {
  padding: 8px 16px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.about-modal__description {
  margin: 0;
  color: $text;
  line-height: 1.5;
  font-size: $text-md;
}

.about-modal__subtitle {
  margin: 8px 0 0;
  font-size: $text-md;
  font-weight: 600;
}

.about-modal__list {
  margin: 0;
  padding-left: 20px;
  color: $text;
  font-size: $text-sm;
  line-height: 1.6;

  li {
    margin-bottom: 4px;
  }

  strong {
    color: var(--gold);
  }
}

.about-modal__footer {
  margin-top: 16px;
  text-align: center;
  font-size: $text-xs;
  border-top: 1px solid var(--border-bronze);
  padding-top: 16px;
}
</style>
