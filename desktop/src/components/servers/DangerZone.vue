<script setup lang="ts">
import { Trash2 } from "@lucide/vue";
import { ref } from "vue";
import { useServersStore } from "../../store/servers";
import BaseButton from "../base/BaseButton.vue";
import ConfirmDialog from "../base/ConfirmDialog.vue";
import HintText from "../base/HintText.vue";

const props = defineProps<{ serverId: string; serverName: string }>();
const emit = defineEmits<{ removed: [] }>();
const serversStore = useServersStore();

const showConfirm = ref(false);
const hint = ref("");

async function confirmDelete(): Promise<void> {
  showConfirm.value = false;
  try {
    await serversStore.removeServerFlow(props.serverId);
    hint.value = "";
    emit("removed");
  } catch (error) {
    hint.value = error instanceof Error ? error.message : String(error);
  }
}
</script>

<template>
  <div class="danger-zone" data-testid="danger-zone">
    <h3 class="danger-zone__title">Zona peligrosa</h3>
    <div class="danger-zone__row">
      <div>
        <div class="danger-zone__label">Eliminar servidor</div>
        <p class="danger-zone__description">
          Se cerrará la conexión y se quitará "{{ serverName }}" de tu lista. Esta acción no se puede deshacer.
        </p>
      </div>
      <BaseButton
        variant="danger tiny"
        :icon="Trash2"
        :icon-size="14"
        test-id="danger-zone-delete-server"
        @click="showConfirm = true"
      >
        Eliminar servidor
      </BaseButton>
    </div>
    <HintText :text="hint" :error="true" test-id="danger-zone-hint" />
  </div>
  <ConfirmDialog
    v-if="showConfirm"
    title="Eliminar servidor"
    :message="`¿Eliminar '${serverName}'? Esta acción no se puede deshacer.`"
    confirm-label="Eliminar"
    danger
    test-id="danger-zone-confirm"
    @confirm="confirmDelete"
    @cancel="showConfirm = false"
  />
</template>

<style scoped lang="scss">
.danger-zone {
  margin-top: $spacing-lg;
  padding-top: $spacing-lg;
  border-top: 1px solid rgba($danger, 0.3);

  &__title {
    margin: 0 0 $spacing-md;
    font-size: $text-md;
    font-weight: 600;
    color: $danger;
    border-bottom: none;
    padding-bottom: 0;
  }

  &__row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: $spacing-md;
  }

  &__label {
    font-weight: 600;
    color: $text;
    margin-bottom: 2px;
  }

  &__description {
    margin: 0;
    font-size: $text-sm;
    color: $text-muted;
    max-width: 32rem;
  }
}
</style>
