<script setup lang="ts">
import { Lock, Users } from "@lucide/vue";
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { errorMessage } from "../../services/http";
import { useCommunities } from "../../stores/communities";
import { useRooms } from "../../stores/rooms";
import { useSession } from "../../stores/session";
import WButton from "../atoms/WButton.vue";
import WInput from "../atoms/WInput.vue";
import WSwitch from "../atoms/WSwitch.vue";
import FormField from "../molecules/FormField.vue";
import WModal from "./WModal.vue";

const props = defineProps<{ communityId?: string | null }>();
const emit = defineEmits<{ close: [] }>();

const rooms = useRooms();
const communities = useCommunities();
const session = useSession();
const router = useRouter();

const name = ref(`Sala de ${session.me?.displayName ?? "Warcraft"}`);
const maxPlayers = ref(12);
const usePassword = ref(false);
const password = ref("");
const communityId = ref<string | null>(props.communityId ?? null);
const error = ref<string | null>(null);
const busy = ref(false);

const nameError = computed(() =>
  name.value.trim().length > 0 && (name.value.trim().length < 3 || name.value.trim().length > 48)
    ? "Entre 3 y 48 caracteres"
    : null,
);

async function submit() {
  if (nameError.value || !name.value.trim()) return;
  busy.value = true;
  error.value = null;
  try {
    const created = await rooms.create({
      name: name.value.trim(),
      communityId: communityId.value,
      maxPlayers: maxPlayers.value,
      password: usePassword.value ? password.value : undefined,
    });
    emit("close");
    void router.push(`/room/${created.room.id}`);
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <WModal title="Crear sala" subtitle="Tus amigos se conectarán directamente a tu partida." @close="emit('close')">
    <form id="create-room" class="create-room" @submit.prevent="submit">
      <FormField label="Nombre de la sala" for="room-name" :error="nameError">
        <WInput id="room-name" v-model="name" :invalid="!!nameError" :maxlength="48" />
      </FormField>

      <div class="create-room__row">
        <FormField label="Jugadores" for="room-players">
          <div class="create-room__players">
            <Users :size="16" />
            <input id="room-players" v-model.number="maxPlayers" type="range" min="2" max="24" />
            <strong>{{ maxPlayers }}</strong>
          </div>
        </FormField>
        <FormField label="Comunidad" for="room-community">
          <select id="room-community" v-model="communityId" class="create-room__select">
            <option :value="null">Pública (todos)</option>
            <option v-for="c in communities.mine" :key="c.id" :value="c.id">{{ c.name }}</option>
          </select>
        </FormField>
      </div>

      <div class="create-room__toggle">
        <Lock :size="16" />
        <div>
          <strong>Sala con contraseña</strong>
          <p>Solo entra quien la conozca (o reciba tu invitación).</p>
        </div>
        <WSwitch v-model="usePassword" label="Sala con contraseña" />
      </div>
      <FormField v-if="usePassword" label="Contraseña" for="room-password">
        <WInput id="room-password" v-model="password" type="password" />
      </FormField>

      <p v-if="error" class="create-room__error" role="alert">{{ error }}</p>
    </form>
    <template #footer>
      <WButton variant="ghost" @click="emit('close')">Cancelar</WButton>
      <WButton type="submit" form="create-room" :loading="busy" :disabled="!name.trim() || !!nameError"
        >Crear sala</WButton
      >
    </template>
  </WModal>
</template>

<style scoped lang="scss">
.create-room {
  display: flex;
  flex-direction: column;
  gap: var(--s-4);

  &__row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--s-4);
  }
  &__players {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    height: 40px;
    color: var(--text-3);
    input {
      flex: 1;
      accent-color: var(--accent);
    }
    strong {
      width: 22px;
      text-align: right;
      color: var(--text-1);
    }
  }
  &__select {
    height: 40px;
    padding: 0 var(--s-3);
    background: var(--bg-1);
    border: 1px solid var(--line-2);
    border-radius: var(--r-sm);
    color: var(--text-1);
  }
  &__toggle {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: var(--s-3) var(--s-4);
    background: var(--bg-3);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    color: var(--text-3);
    div {
      flex: 1;
    }
    strong {
      color: var(--text-1);
      font-weight: 600;
    }
    p {
      margin: 2px 0 0;
      font-size: var(--fs-sm);
    }
  }
  &__error {
    margin: 0;
    color: var(--danger);
  }
}
</style>
