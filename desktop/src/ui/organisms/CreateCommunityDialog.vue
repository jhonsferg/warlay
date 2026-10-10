<script setup lang="ts">
import { Globe, Lock } from "@lucide/vue";
import { ref } from "vue";
import { useRouter } from "vue-router";
import { errorMessage } from "../../services/http";
import { useCommunities } from "../../stores/communities";
import WAvatar from "../atoms/WAvatar.vue";
import WButton from "../atoms/WButton.vue";
import WInput from "../atoms/WInput.vue";
import FormField from "../molecules/FormField.vue";
import WModal from "./WModal.vue";

const emit = defineEmits<{ close: [] }>();
const communities = useCommunities();
const router = useRouter();

const name = ref("");
const description = ref("");
const isPublic = ref(true);
const busy = ref(false);
const error = ref<string | null>(null);

async function submit() {
  busy.value = true;
  error.value = null;
  try {
    const created = await communities.create({
      name: name.value.trim(),
      description: description.value.trim(),
      isPublic: isPublic.value,
    });
    emit("close");
    void router.push(`/c/${created.id}`);
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <WModal title="Crear comunidad" subtitle="Un lugar para tu clan: chat, salas y miembros." @close="emit('close')">
    <form id="create-community" class="create-community" @submit.prevent="submit">
      <div class="create-community__preview">
        <WAvatar :name="name || 'Nueva comunidad'" color="#7b61ff" size="xl" shape="squircle" />
      </div>
      <FormField label="Nombre" for="community-name" hint="Entre 3 y 40 caracteres.">
        <WInput id="community-name" v-model="name" placeholder="Los Orcos de Durotar" :maxlength="40" />
      </FormField>
      <FormField label="Descripción" for="community-description">
        <textarea
          id="community-description"
          v-model="description"
          class="create-community__textarea"
          rows="3"
          maxlength="300"
          placeholder="¿De qué se trata tu comunidad?"
        />
      </FormField>
      <div class="create-community__visibility" role="radiogroup" aria-label="Visibilidad">
        <button
          type="button"
          role="radio"
          :aria-checked="isPublic"
          :class="{ active: isPublic }"
          @click="isPublic = true"
        >
          <Globe :size="18" />
          <strong>Pública</strong>
          <span>Cualquiera puede encontrarla y unirse.</span>
        </button>
        <button
          type="button"
          role="radio"
          :aria-checked="!isPublic"
          :class="{ active: !isPublic }"
          @click="isPublic = false"
        >
          <Lock :size="18" />
          <strong>Privada</strong>
          <span>Solo por invitación.</span>
        </button>
      </div>
      <p v-if="error" class="create-community__error" role="alert">{{ error }}</p>
    </form>
    <template #footer>
      <WButton variant="ghost" @click="emit('close')">Cancelar</WButton>
      <WButton type="submit" form="create-community" :loading="busy" :disabled="name.trim().length < 3"
        >Crear comunidad</WButton
      >
    </template>
  </WModal>
</template>

<style scoped lang="scss">
.create-community {
  display: flex;
  flex-direction: column;
  gap: var(--s-4);

  &__preview {
    display: flex;
    justify-content: center;
  }
  &__textarea {
    resize: vertical;
    padding: var(--s-3);
    background: var(--bg-1);
    border: 1px solid var(--line-2);
    border-radius: var(--r-sm);
    color: var(--text-1);
    &:focus {
      outline: none;
      border-color: var(--accent);
      box-shadow: 0 0 0 3px var(--accent-soft);
    }
  }
  &__visibility {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--s-3);

    button {
      @include reset-button;
      @include interactive;
      display: flex;
      flex-direction: column;
      gap: 4px;
      padding: var(--s-4);
      text-align: left;
      background: var(--bg-3);
      border: 1px solid var(--line-2);
      border-radius: var(--r-md);
      color: var(--text-3);
      strong {
        color: var(--text-1);
      }
      span {
        font-size: var(--fs-sm);
      }
      &.active {
        border-color: var(--accent);
        background: var(--accent-soft);
        color: #c4b8ff;
      }
    }
  }
  &__error {
    margin: 0;
    color: var(--danger);
  }
}
</style>
