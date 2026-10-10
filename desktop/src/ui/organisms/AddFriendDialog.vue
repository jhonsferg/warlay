<script setup lang="ts">
import { Search, UserPlus } from "@lucide/vue";
import { ref, watch } from "vue";
import { errorMessage } from "../../services/http";
import type { UserWithPresence } from "../../services/types";
import { useSocial } from "../../stores/social";
import { useToasts } from "../../stores/toasts";
import WButton from "../atoms/WButton.vue";
import WInput from "../atoms/WInput.vue";
import UserChip from "../molecules/UserChip.vue";
import WModal from "./WModal.vue";

const emit = defineEmits<{ close: [] }>();
const social = useSocial();
const toasts = useToasts();

const query = ref("");
const results = ref<UserWithPresence[]>([]);
const sent = ref<Set<string>>(new Set());
let timer: ReturnType<typeof setTimeout> | null = null;

watch(query, (q) => {
  if (timer) clearTimeout(timer);
  timer = setTimeout(async () => {
    results.value = q.trim().length >= 2 ? await social.search(q.trim()).catch(() => []) : [];
  }, 250);
});

const isFriend = (id: string) => !!social.friend(id);
const isPending = (id: string) => sent.value.has(id) || social.outgoing.some((r) => r.to.id === id);

async function add(user: UserWithPresence) {
  try {
    const status = await social.sendRequest(user.username);
    sent.value.add(user.id);
    toasts.success(
      status === "accepted" ? `Ahora eres amigo de ${user.displayName}` : "Solicitud enviada",
      `@${user.username}`,
    );
  } catch (e) {
    toasts.error("No se pudo enviar la solicitud", errorMessage(e));
  }
}
</script>

<template>
  <WModal title="Agregar amigo" subtitle="Busca por nombre de usuario." @close="emit('close')">
    <WInput v-model="query" placeholder="Buscar jugadores..." size="lg" aria-label="Buscar jugadores">
      <template #icon><Search /></template>
    </WInput>
    <div class="add-friend__results">
      <p v-if="query.trim().length >= 2 && results.length === 0" class="add-friend__empty">
        Sin resultados para “{{ query }}”.
      </p>
      <div v-for="u in results" :key="u.id" class="add-friend__row" :data-result="u.username">
        <UserChip :user="u" :subtitle="`@${u.username}`" />
        <WButton v-if="isFriend(u.id)" size="sm" variant="ghost" disabled>Amigos</WButton>
        <WButton v-else-if="isPending(u.id)" size="sm" variant="secondary" disabled>Pendiente</WButton>
        <WButton v-else size="sm" @click="add(u)"
          ><template #icon><UserPlus /></template>Agregar</WButton
        >
      </div>
    </div>
  </WModal>
</template>

<style scoped lang="scss">
.add-friend {
  &__results {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 340px;
    overflow-y: auto;
  }
  &__row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-3);
    padding: var(--s-2);
    border-radius: var(--r-md);
    &:hover {
      background: var(--bg-3);
    }
  }
  &__empty {
    color: var(--text-3);
    text-align: center;
    margin: var(--s-4) 0;
  }
}
</style>
