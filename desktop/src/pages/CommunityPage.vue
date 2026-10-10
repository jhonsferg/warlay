<script setup lang="ts">
import { Hash, Plus, Swords, Users } from "@lucide/vue";
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { communities as communitiesApi } from "../services/api";
import { errorMessage } from "../services/http";
import type { CommunityMember, RoomView } from "../services/types";
import { useCommunities } from "../stores/communities";
import { useDialogs } from "../stores/dialogs";
import { useRooms } from "../stores/rooms";
import { useSession } from "../stores/session";
import { useToasts } from "../stores/toasts";
import WBadge from "../ui/atoms/WBadge.vue";
import WButton from "../ui/atoms/WButton.vue";
import EmptyState from "../ui/molecules/EmptyState.vue";
import FriendRow from "../ui/molecules/FriendRow.vue";
import RoomCard from "../ui/molecules/RoomCard.vue";
import SectionHeader from "../ui/molecules/SectionHeader.vue";
import ChatPanel from "../ui/organisms/ChatPanel.vue";
import CommunitySidebar from "../ui/organisms/CommunitySidebar.vue";
import InviteFriendsDialog from "../ui/organisms/InviteFriendsDialog.vue";
import AppShell from "../ui/templates/AppShell.vue";

const route = useRoute();
const router = useRouter();
const communities = useCommunities();
const rooms = useRooms();
const session = useSession();
const dialogs = useDialogs();
const toasts = useToasts();

const id = computed(() => route.params.id as string);
const view = computed(() => (route.query.view as string) || "chat");
const community = computed(() => communities.details[id.value]);
const inviting = ref(false);
const error = ref<string | null>(null);

watch(
  id,
  async (next) => {
    error.value = null;
    try {
      await communities.loadDetail(next);
    } catch (e) {
      error.value = errorMessage(e);
    }
  },
  { immediate: true },
);

const roleLabel = (m: CommunityMember) => (m.role === "owner" ? "Dueño" : m.role === "admin" ? "Admin" : null);

async function joinRoom(room: RoomView) {
  try {
    await rooms.join(room.id);
    void router.push(`/room/${room.id}`);
  } catch (e) {
    toasts.error("No se pudo entrar a la sala", errorMessage(e));
  }
}

async function leave() {
  try {
    await communities.leave(id.value);
    void router.push("/");
  } catch (e) {
    toasts.error("No se pudo salir", errorMessage(e));
  }
}

async function remove() {
  if (!confirm(`¿Eliminar ${community.value?.name}? Esta acción no se puede deshacer.`)) return;
  try {
    await communities.remove(id.value);
    void router.push("/");
  } catch (e) {
    toasts.error("No se pudo eliminar", errorMessage(e));
  }
}

const select = (next: string) => router.replace({ query: { view: next } });
const inviteToCommunity = (userId: string) => communitiesApi.invite(id.value, userId);
</script>

<template>
  <AppShell
    :title="view === 'chat' ? 'general' : view === 'rooms' ? 'salas' : 'miembros'"
    :subtitle="community?.description || community?.name"
    :flush="view === 'chat'"
  >
    <template #sidebar>
      <CommunitySidebar
        v-if="community"
        :community="community"
        :view="view"
        @select="select"
        @invite="inviting = true"
        @leave="leave"
        @remove="remove"
      />
    </template>
    <template #title-icon>
      <Hash v-if="view === 'chat'" :size="20" />
      <Swords v-else-if="view === 'rooms'" :size="20" />
      <Users v-else :size="20" />
    </template>
    <template #actions>
      <WButton v-if="community?.myRole && view === 'rooms'" size="sm" @click="dialogs.openCreateRoom(id)"
        ><template #icon><Plus /></template>Crear sala</WButton
      >
    </template>

    <EmptyState v-if="error" title="No puedes ver esta comunidad" :text="error" />
    <template v-else-if="community">
      <template v-if="view === 'chat'">
        <ChatPanel
          v-if="community.myRole"
          kind="community"
          :target-id="community.id"
          :placeholder="`Mensaje en #general de ${community.name}`"
        />
        <EmptyState v-else title="Únete para participar" text="El chat es solo para miembros.">
          <WButton @click="communities.join(community.id)">Unirme</WButton>
        </EmptyState>
      </template>

      <template v-else-if="view === 'rooms'">
        <SectionHeader title="Salas de la comunidad" :count="community.rooms.length" />
        <div v-if="community.rooms.length" class="community__grid">
          <RoomCard
            v-for="room in community.rooms"
            :key="room.id"
            :room="room"
            :current="rooms.current?.id === room.id"
            @join="joinRoom"
            @open="router.push(`/room/${room.id}`)"
          />
        </div>
        <EmptyState v-else title="No hay salas activas" text="Crea una sala solo para los miembros de esta comunidad.">
          <template #icon><Swords /></template>
          <WButton v-if="community.myRole" @click="dialogs.openCreateRoom(id)"
            ><template #icon><Plus /></template>Crear sala</WButton
          >
        </EmptyState>
      </template>

      <template v-else>
        <SectionHeader title="Miembros" :count="community.members.length" />
        <div class="community__members">
          <FriendRow v-for="m in community.members" :key="m.id" :friend="m">
            <template #actions>
              <WBadge v-if="roleLabel(m)" tone="warning">{{ roleLabel(m) }}</WBadge>
              <WBadge v-if="m.id === session.me?.id" tone="accent">Tú</WBadge>
            </template>
          </FriendRow>
        </div>
      </template>
    </template>

    <InviteFriendsDialog
      v-if="inviting && community"
      :title="`Invitar a ${community.name}`"
      :exclude="community.members.map((m) => m.id)"
      :invite="inviteToCommunity"
      @close="inviting = false"
    />
  </AppShell>
</template>

<style scoped lang="scss">
.community {
  &__grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--s-4);
  }
  &__members {
    max-width: 760px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    --dot-ring: var(--bg-1);
    :deep(.friend-row__actions) {
      opacity: 1;
    }
  }
}
</style>
