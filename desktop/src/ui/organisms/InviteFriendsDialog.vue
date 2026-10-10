<script setup lang="ts">
import { Check } from "@lucide/vue";
import { computed, ref } from "vue";
import { errorMessage } from "../../services/http";
import { useSocial } from "../../stores/social";
import { useToasts } from "../../stores/toasts";
import WButton from "../atoms/WButton.vue";
import FriendRow from "../molecules/FriendRow.vue";
import EmptyState from "../molecules/EmptyState.vue";
import WModal from "./WModal.vue";

const props = defineProps<{ title: string; exclude: string[]; invite: (userId: string) => Promise<unknown> }>();
const emit = defineEmits<{ close: [] }>();
const social = useSocial();
const toasts = useToasts();
const invited = ref<Set<string>>(new Set());

const candidates = computed(() => social.sortedFriends.filter((f) => !props.exclude.includes(f.id)));

async function send(userId: string) {
  try {
    await props.invite(userId);
    invited.value = new Set([...invited.value, userId]);
  } catch (e) {
    toasts.error("No se pudo invitar", errorMessage(e));
  }
}
</script>

<template>
  <WModal :title="title" subtitle="Tus amigos recibirán la invitación al instante." @close="emit('close')">
    <EmptyState
      v-if="candidates.length === 0"
      title="No hay a quién invitar"
      text="Todos tus amigos ya están aquí, o aún no agregaste ninguno."
    />
    <div class="invite__list">
      <FriendRow v-for="f in candidates" :key="f.id" :friend="f">
        <template #actions>
          <WButton v-if="invited.has(f.id)" size="sm" variant="ghost" disabled
            ><template #icon><Check /></template>Invitado</WButton
          >
          <WButton v-else size="sm" @click="send(f.id)">Invitar</WButton>
        </template>
      </FriendRow>
    </div>
  </WModal>
</template>

<style scoped lang="scss">
.invite__list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 380px;
  overflow-y: auto;
  :deep(.friend-row__actions) {
    opacity: 1;
  }
}
</style>
