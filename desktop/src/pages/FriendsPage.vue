<script setup lang="ts">
import { MessageCircle, UserPlus, UserRound, Users } from "@lucide/vue";
import { computed, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { errorMessage } from "../services/http";
import { useDialogs } from "../stores/dialogs";
import { useRooms } from "../stores/rooms";
import { useSocial } from "../stores/social";
import { useToasts } from "../stores/toasts";
import WButton from "../ui/atoms/WButton.vue";
import EmptyState from "../ui/molecules/EmptyState.vue";
import UserChip from "../ui/molecules/UserChip.vue";
import ChatPanel from "../ui/organisms/ChatPanel.vue";
import FriendProfilePanel from "../ui/organisms/FriendProfilePanel.vue";
import FriendsList from "../ui/organisms/FriendsList.vue";
import HomeSidebar from "../ui/organisms/HomeSidebar.vue";
import AppShell from "../ui/templates/AppShell.vue";

/**
 * Friends, Messenger-desktop style: every friend on the left (connected or
 * not), the conversation on the right as soon as you click one, and a profile
 * panel on demand. The selected friend lives in the URL (`/friends/:id`).
 */
const route = useRoute();
const router = useRouter();
const social = useSocial();
const rooms = useRooms();
const dialogs = useDialogs();
const toasts = useToasts();

const selectedId = computed(() => (route.params.id as string | undefined) || undefined);
const friend = computed(() => (selectedId.value ? social.friend(selectedId.value) : undefined));
const profileOpen = ref(false);

const open = (id: string) => router.push(`/friends/${id}`);

async function invite() {
  if (!friend.value) return;
  try {
    await rooms.invite(friend.value.id);
    toasts.success("Invitación enviada", friend.value.displayName);
  } catch (e) {
    toasts.error("No se pudo invitar", e instanceof Error ? e.message : String(e));
  }
}

/** Confirm, remove, and leave the conversation of someone who is no longer a friend. */
async function removeFriend() {
  const target = friend.value;
  if (!target) return;
  const accepted = await dialogs.confirm({
    title: `Eliminar a ${target.displayName} de tus amigos`,
    message: `${target.displayName} dejará de aparecer en tu lista de amigos.`,
    bullets: [
      "Ya no podrán escribirse en privado ni ver cuándo está conectado.",
      "Para volver a ser amigos tendrán que enviarse una nueva solicitud.",
    ],
    confirmLabel: "Eliminar amigo",
    tone: "danger",
  });
  if (!accepted) return;
  try {
    await social.remove(target.id);
    profileOpen.value = false;
    void router.replace("/friends");
    toasts.success("Amigo eliminado", target.displayName);
  } catch (e) {
    toasts.error("No se pudo eliminar", errorMessage(e));
  }
}
</script>

<template>
  <AppShell title="Amigos" :subtitle="`${social.onlineCount} en línea`" flush>
    <template #sidebar><HomeSidebar /></template>
    <template #title-icon><Users :size="20" /></template>
    <template #actions>
      <WButton size="sm" data-testid="add-friend" @click="dialogs.addFriend = true"
        ><template #icon><UserPlus /></template>Agregar amigo</WButton
      >
    </template>

    <div class="friends">
      <div class="friends__list"><FriendsList :selected-id="selectedId" @select="open" /></div>

      <section class="friends__main" aria-label="Conversación">
        <template v-if="friend">
          <header class="friends__bar" data-testid="chat-header">
            <UserChip :user="friend" :presence="friend.presence" />
            <WButton
              v-if="rooms.current && !rooms.current.members.some((m) => m.id === friend!.id)"
              size="sm"
              variant="secondary"
              @click="invite"
              ><template #icon><UserPlus /></template>Invitar a mi sala</WButton
            >
            <WButton
              size="sm"
              :variant="profileOpen ? 'primary' : 'secondary'"
              data-testid="profile-toggle"
              @click="profileOpen = !profileOpen"
              ><template #icon><UserRound /></template>Ver perfil</WButton
            >
          </header>
          <ChatPanel
            :key="friend.id"
            kind="dm"
            :target-id="friend.id"
            :placeholder="`Mensaje para @${friend.username}`"
            :empty-title="`Esto es el comienzo de tu chat con ${friend.displayName}`"
          />
        </template>
        <EmptyState
          v-else-if="selectedId && social.loaded"
          title="Este chat no está disponible"
          text="Solo puedes escribir en privado a tus amigos."
        >
          <template #icon><MessageCircle /></template>
        </EmptyState>
        <EmptyState
          v-else
          title="Elige un amigo para chatear"
          text="Haz clic en cualquiera de tus amigos, esté conectado o no, para abrir la conversación."
          data-testid="friends-pick"
        >
          <template #icon><MessageCircle /></template>
        </EmptyState>
      </section>

      <FriendProfilePanel v-if="friend && profileOpen" :friend="friend" @close="profileOpen = false" @remove="removeFriend" />
    </div>
  </AppShell>
</template>

<style scoped lang="scss">
.friends {
  display: flex;
  height: 100%;
  min-height: 0;

  &__list {
    width: 340px;
    flex-shrink: 0;
    border-right: 1px solid var(--line);
    background: var(--bg-2);
  }
  &__main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  &__bar {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: var(--s-2) var(--s-4);
    border-bottom: 1px solid var(--line);
    :deep(.user-chip) {
      flex: 1;
    }
  }
}
</style>
