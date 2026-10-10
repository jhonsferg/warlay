<script setup lang="ts">
import { AtSign, UserPlus } from "@lucide/vue";
import { computed } from "vue";
import { useRoute } from "vue-router";
import { useRooms } from "../stores/rooms";
import { useSocial } from "../stores/social";
import { useToasts } from "../stores/toasts";
import WButton from "../ui/atoms/WButton.vue";
import EmptyState from "../ui/molecules/EmptyState.vue";
import ChatPanel from "../ui/organisms/ChatPanel.vue";
import HomeSidebar from "../ui/organisms/HomeSidebar.vue";
import AppShell from "../ui/templates/AppShell.vue";
import { presenceLabel } from "../ui/presence";

const route = useRoute();
const social = useSocial();
const rooms = useRooms();
const toasts = useToasts();
const friend = computed(() => social.friend(route.params.id as string));

async function invite() {
  if (!friend.value) return;
  try {
    await rooms.invite(friend.value.id);
    toasts.success("Invitación enviada", friend.value.displayName);
  } catch (e) {
    toasts.error("No se pudo invitar", e instanceof Error ? e.message : String(e));
  }
}
</script>

<template>
  <AppShell
    :title="friend?.displayName ?? 'Mensajes'"
    :subtitle="friend ? presenceLabel(friend.presence.status) : undefined"
    flush
  >
    <template #sidebar><HomeSidebar /></template>
    <template #title-icon><AtSign :size="20" /></template>
    <template #actions>
      <WButton
        v-if="friend && rooms.current && !rooms.current.members.some((m) => m.id === friend!.id)"
        size="sm"
        variant="secondary"
        @click="invite"
        ><template #icon><UserPlus /></template>Invitar a mi sala</WButton
      >
    </template>
    <EmptyState
      v-if="!friend && social.loaded"
      title="Este chat no está disponible"
      text="Solo puedes escribir en privado a tus amigos."
    />
    <ChatPanel
      v-else-if="friend"
      :key="friend.id"
      kind="dm"
      :target-id="friend.id"
      :placeholder="`Mensaje para @${friend.username}`"
      :empty-title="`Esto es el comienzo de tu chat con ${friend.displayName}`"
    />
  </AppShell>
</template>
