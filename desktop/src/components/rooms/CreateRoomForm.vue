<script setup lang="ts">
import { toTypedSchema } from "@vee-validate/zod";
import { DoorOpen, X } from "@lucide/vue";
import { computed, ref } from "vue";
import { useForm } from "vee-validate";
import { z } from "zod";
import { useRoomsStore } from "../../store/rooms";
import { useServersStore } from "../../store/servers";
import { gamePortSchema, roomNameSchema } from "../../utils/validation";
import BaseButton from "../base/BaseButton.vue";
import BaseInput from "../base/BaseInput.vue";
import BaseNumberInput from "../base/BaseNumberInput.vue";
import HintText from "../base/HintText.vue";

const props = defineProps<{ serverId: string }>();
const emit = defineEmits<{ created: []; cancel: [] }>();
const serversStore = useServersStore();
const roomsStore = useRoomsStore();

const { defineField, errors, handleSubmit, resetForm } = useForm({
  validationSchema: toTypedSchema(z.object({ name: roomNameSchema, gamePort: gamePortSchema })),
  initialValues: { name: "", gamePort: 6112 },
});
const [name] = defineField("name");
const [gamePort] = defineField("gamePort");
const password = ref("");
const submitError = ref("");

const connection = computed(() => serversStore.serverConnections.get(props.serverId));
const disabled = computed(
  () => connection.value?.status !== "connected" || !connection.value?.authenticated || roomsStore.activeRoom !== null,
);
const hint = computed(
  () => errors.value.name || errors.value.gamePort || submitError.value || connection.value?.errorMessage || "",
);

const submit = handleSubmit(async (values) => {
  submitError.value = "";
  try {
    await roomsStore.createRoom(
      props.serverId,
      values.name.trim(),
      values.gamePort,
      password.value.trim() || undefined,
    );
    resetForm();
    password.value = "";
    emit("created");
  } catch (error) {
    submitError.value = `Error al fundar la sala: ${String(error)}`;
  }
});
</script>

<template>
  <div class="create-room-form">
    <label class="field"
      >Nombre de la sala<BaseInput
        v-model="name"
        placeholder="La Cima del Trueno"
        test-id="create-room-name-input"
        aria-label="Nombre de la sala"
    /></label>
    <label class="field"
      >Puerto de juego<BaseNumberInput
        v-model="gamePort"
        :min="1"
        :max="65535"
        test-id="create-room-port-input"
        aria-label="Puerto de juego"
    /></label>
    <label class="field"
      >Contraseña (opcional)<BaseInput
        v-model="password"
        placeholder="Dejar vacío para sala abierta"
        test-id="create-room-password-input"
        aria-label="Contraseña de la sala"
    /></label>
    <HintText :text="hint" :error="hint.length > 0" test-id="create-room-hint" />
    <div class="create-room-form__actions">
      <BaseButton variant="secondary" :icon="X" :icon-size="14" test-id="create-room-cancel" @click="emit('cancel')">
        Cancelar
      </BaseButton>
      <BaseButton
        variant="primary"
        :icon="DoorOpen"
        :icon-size="14"
        :disabled="disabled"
        test-id="create-room-submit"
        @click="submit"
      >
        Fundar sala
      </BaseButton>
    </div>
  </div>
</template>

<style scoped lang="scss">
.create-room-form {
  display: flex;
  flex-direction: column;
  gap: $spacing-md;

  &__actions {
    display: flex;
    justify-content: flex-end;
    gap: $spacing-sm;
  }
}
</style>
