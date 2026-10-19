<script setup lang="ts">
import { Gamepad2, Plus, Radio, Search, Swords, UserPlus } from "@lucide/vue";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { ApiError, errorMessage } from "../services/http";
import type { RoomView } from "../services/types";
import { useCommunities } from "../stores/communities";
import { useDialogs } from "../stores/dialogs";
import { useRooms } from "../stores/rooms";
import { useSession } from "../stores/session";
import { useSocial } from "../stores/social";
import { useToasts } from "../stores/toasts";
import WButton from "../ui/atoms/WButton.vue";
import WInput from "../ui/atoms/WInput.vue";
import AvatarStack from "../ui/molecules/AvatarStack.vue";
import EmptyState from "../ui/molecules/EmptyState.vue";
import RoomCard from "../ui/molecules/RoomCard.vue";
import SectionHeader from "../ui/molecules/SectionHeader.vue";
import TabBar from "../ui/molecules/TabBar.vue";
import HomeSidebar from "../ui/organisms/HomeSidebar.vue";
import PasswordDialog from "../ui/organisms/PasswordDialog.vue";
import AppShell from "../ui/templates/AppShell.vue";

const rooms = useRooms();
const social = useSocial();
const session = useSession();
const communities = useCommunities();
const dialogs = useDialogs();
const toasts = useToasts();
const router = useRouter();

type Filter = "all" | "friends" | "communities" | "open";
const filter = ref<Filter>("all");
const query = ref("");
const joining = ref<string | null>(null);
const passwordFor = ref<RoomView | null>(null);
const passwordError = ref<string | null>(null);

const friendIds = computed(() => new Set(social.friends.map((f) => f.id)));
const myCommunities = computed(() => new Set(communities.mine.map((c) => c.id)));

const visible = computed(() =>
  rooms.list
    .filter((r) => r.status !== "closed")
    .filter((r) => {
      if (filter.value === "friends") return r.members.some((m) => friendIds.value.has(m.id));
      if (filter.value === "communities") return !!r.communityId && myCommunities.value.has(r.communityId);
      if (filter.value === "open") return r.status === "open" && !r.hasPassword && r.members.length < r.maxPlayers;
      return true;
    })
    .filter((r) => !query.value.trim() || r.name.toLowerCase().includes(query.value.trim().toLowerCase())),
);

const friendsPlaying = computed(() =>
  social.friends.filter((f) => f.presence.status === "in_game" || f.presence.status === "in_room"),
);

async function join(room: RoomView, password?: string) {
  if (room.hasPassword && password === undefined) {
    passwordFor.value = room;
    passwordError.value = null;
    return;
  }
  joining.value = room.id;
  try {
    await rooms.join(room.id, password);
    passwordFor.value = null;
    void router.push(`/room/${room.id}`);
  } catch (e) {
    if (e instanceof ApiError && e.code === "wrong_password") passwordError.value = e.message;
    else toasts.error("No se pudo entrar a la sala", errorMessage(e));
  } finally {
    joining.value = null;
  }
}

// The server lists a limited number of rooms: search there too, so a room
// past that limit can still be found by name.
let searchTimer: ReturnType<typeof setTimeout> | null = null;
watch(query, (q) => {
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => void rooms.loadList(q).catch(() => {}), 300);
});
onUnmounted(() => {
  if (searchTimer) clearTimeout(searchTimer);
});

onMounted(() => void rooms.loadList(query.value).catch(() => {}));
</script>

<template>
  <AppShell title="Salas" subtitle="Encuentra una partida o crea la tuya">
    <template #sidebar><HomeSidebar /></template>
    <template #title-icon><Swords :size="20" /></template>

    <section class="hero" :class="{ 'hero--in-room': rooms.current }">
      <div class="hero__copy">
        <p class="hero__eyebrow"><Radio :size="14" /> Conexión directa entre jugadores</p>
        <template v-if="rooms.current">
          <h2>
            Estás en <span>{{ rooms.current.name }}</span>
          </h2>
          <p>
            {{ rooms.current.members.length }} de {{ rooms.current.maxPlayers }} jugadores. Lanza el juego cuando estén
            todos.
          </p>
        </template>
        <template v-else>
          <h2>Hola, {{ session.me?.displayName }}</h2>
          <p>Crea una sala y tus amigos se conectarán directamente a tu partida, sin servidores en medio.</p>
        </template>
        <div class="hero__actions">
          <WButton v-if="rooms.current" size="lg" @click="router.push(`/room/${rooms.current.id}`)">
            <template #icon><Gamepad2 /></template>Ir a mi sala
          </WButton>
          <WButton v-else size="lg" data-testid="create-room" @click="dialogs.openCreateRoom()">
            <template #icon><Plus /></template>Crear sala
          </WButton>
          <WButton size="lg" variant="secondary" @click="dialogs.addFriend = true">
            <template #icon><UserPlus /></template>Agregar amigos
          </WButton>
        </div>
      </div>
      <div v-if="friendsPlaying.length" class="hero__friends">
        <p class="hero__eyebrow">Amigos jugando</p>
        <AvatarStack :users="friendsPlaying" :max="6" />
      </div>
    </section>

    <SectionHeader title="Salas activas" :count="visible.length">
      <WInput v-model="query" placeholder="Buscar sala..." aria-label="Buscar sala" class="lobby__search">
        <template #icon><Search /></template>
      </WInput>
      <TabBar
        v-model="filter"
        :tabs="[
          { id: 'all', label: 'Todas' },
          { id: 'open', label: 'Abiertas' },
          { id: 'friends', label: 'Con amigos' },
          { id: 'communities', label: 'Mis comunidades' },
        ]"
      />
    </SectionHeader>

    <div v-if="visible.length" class="lobby__grid">
      <RoomCard
        v-for="room in visible"
        :key="room.id"
        :room="room"
        :current="rooms.current?.id === room.id"
        :busy="joining === room.id"
        @join="join($event)"
        @open="router.push(`/room/${room.id}`)"
      />
    </div>
    <EmptyState v-else title="No hay salas por aquí" text="Sé el primero: crea una sala e invita a tus amigos.">
      <template #icon><Swords /></template>
      <WButton @click="dialogs.openCreateRoom()"
        ><template #icon><Plus /></template>Crear sala</WButton
      >
    </EmptyState>

    <PasswordDialog
      v-if="passwordFor"
      :room-name="passwordFor.name"
      :error="passwordError"
      :busy="joining === passwordFor.id"
      @close="passwordFor = null"
      @submit="join(passwordFor!, $event)"
    />
  </AppShell>
</template>

<style scoped lang="scss">
.hero {
  position: relative;
  overflow: hidden;
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: var(--s-6);
  padding: var(--s-8);
  margin-bottom: var(--s-8);
  border-radius: var(--r-xl);
  border: 1px solid var(--line-2);
  background:
    radial-gradient(600px 300px at 100% 0%, rgba(34, 211, 238, 0.18), transparent 60%),
    radial-gradient(700px 360px at 0% 100%, rgba(123, 97, 255, 0.3), transparent 60%),
    linear-gradient(135deg, #141130, #0c1424);

  &--in-room {
    border-color: rgba(251, 191, 36, 0.35);
  }

  &__eyebrow {
    @include eyebrow;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--accent-2);
    margin: 0 0 var(--s-2);
  }
  &__copy {
    max-width: 560px;
    h2 {
      @include display-font(40px);
      margin: 0 0 var(--s-2);
      span {
        color: var(--in-room);
      }
    }
    > p:not(.hero__eyebrow) {
      color: var(--text-2);
      margin: 0;
      font-size: var(--fs-lg);
    }
  }
  &__actions {
    display: flex;
    gap: var(--s-3);
    margin-top: var(--s-6);
  }
  &__friends {
    text-align: right;
    .avatar-stack {
      justify-content: flex-end;
    }
  }
}

.lobby {
  &__search {
    width: 220px;
  }
  &__grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--s-4);
  }
}
</style>
