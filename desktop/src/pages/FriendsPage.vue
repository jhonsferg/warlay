<script setup lang="ts">
import { Check, MessageCircle, UserMinus, UserPlus, Users, X } from "@lucide/vue";
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { errorMessage } from "../services/http";
import { useDialogs } from "../stores/dialogs";
import { useSocial } from "../stores/social";
import { useToasts } from "../stores/toasts";
import WButton from "../ui/atoms/WButton.vue";
import WIconButton from "../ui/atoms/WIconButton.vue";
import EmptyState from "../ui/molecules/EmptyState.vue";
import FriendRow from "../ui/molecules/FriendRow.vue";
import TabBar from "../ui/molecules/TabBar.vue";
import UserChip from "../ui/molecules/UserChip.vue";
import HomeSidebar from "../ui/organisms/HomeSidebar.vue";
import AppShell from "../ui/templates/AppShell.vue";

const social = useSocial();
const dialogs = useDialogs();
const toasts = useToasts();
const router = useRouter();

type Tab = "online" | "all" | "pending";
const tab = ref<Tab>("online");

const shown = computed(() =>
  tab.value === "online" ? social.sortedFriends.filter((f) => f.presence.status !== "offline") : social.sortedFriends,
);

async function act(fn: () => Promise<unknown>, failure: string) {
  try {
    await fn();
  } catch (e) {
    toasts.error(failure, errorMessage(e));
  }
}
</script>

<template>
  <AppShell title="Amigos" :subtitle="`${social.onlineCount} en línea`">
    <template #sidebar><HomeSidebar /></template>
    <template #title-icon><Users :size="20" /></template>
    <template #actions>
      <WButton size="sm" data-testid="add-friend" @click="dialogs.addFriend = true"
        ><template #icon><UserPlus /></template>Agregar amigo</WButton
      >
    </template>

    <TabBar
      v-model="tab"
      class="friends__tabs"
      :tabs="[
        { id: 'online', label: 'En línea' },
        { id: 'all', label: 'Todos' },
        { id: 'pending', label: 'Pendientes', count: social.incoming.length },
      ]"
    />

    <template v-if="tab === 'pending'">
      <EmptyState
        v-if="!social.incoming.length && !social.outgoing.length"
        title="Sin solicitudes pendientes"
        text="Cuando alguien te agregue aparecerá aquí."
      >
        <template #icon><UserPlus /></template>
      </EmptyState>
      <section v-if="social.incoming.length" class="friends__section">
        <p class="friends__eyebrow">Recibidas — {{ social.incoming.length }}</p>
        <div v-for="r in social.incoming" :key="r.id" class="friends__request" :data-request="r.from.username">
          <UserChip :user="r.from" :subtitle="`@${r.from.username} quiere ser tu amigo`" />
          <WButton size="sm" variant="success" @click="act(() => social.accept(r.id), 'No se pudo aceptar')"
            ><template #icon><Check /></template>Aceptar</WButton
          >
          <WIconButton label="Rechazar" tone="danger" @click="act(() => social.decline(r.id), 'No se pudo rechazar')"
            ><X
          /></WIconButton>
        </div>
      </section>
      <section v-if="social.outgoing.length" class="friends__section">
        <p class="friends__eyebrow">Enviadas — {{ social.outgoing.length }}</p>
        <div v-for="r in social.outgoing" :key="r.id" class="friends__request">
          <UserChip :user="r.to" :subtitle="`@${r.to.username} · esperando respuesta`" />
          <WButton size="sm" variant="ghost" @click="act(() => social.decline(r.id), 'No se pudo cancelar')"
            >Cancelar</WButton
          >
        </div>
      </section>
    </template>

    <template v-else>
      <EmptyState
        v-if="shown.length === 0"
        :title="tab === 'online' ? 'Nadie en línea ahora' : 'Aún no tienes amigos'"
        text="Busca a tus compañeros por su nombre de usuario."
      >
        <template #icon><Users /></template>
        <WButton @click="dialogs.addFriend = true"
          ><template #icon><UserPlus /></template>Agregar amigo</WButton
        >
      </EmptyState>
      <div class="friends__list">
        <FriendRow v-for="f in shown" :key="f.id" :friend="f">
          <template #actions>
            <WIconButton label="Enviar mensaje" @click="router.push(`/dm/${f.id}`)"><MessageCircle /></WIconButton>
            <WIconButton
              label="Eliminar amigo"
              tone="danger"
              @click="act(() => social.remove(f.id), 'No se pudo eliminar')"
              ><UserMinus
            /></WIconButton>
          </template>
        </FriendRow>
      </div>
    </template>
  </AppShell>
</template>

<style scoped lang="scss">
.friends {
  &__tabs {
    margin-bottom: var(--s-6);
  }
  &__list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-width: 820px;
    --dot-ring: var(--bg-1);
  }
  &__section {
    max-width: 820px;
    margin-bottom: var(--s-6);
  }
  &__eyebrow {
    @include eyebrow;
    margin: 0 0 var(--s-2);
  }
  &__request {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: var(--s-3);
    border-radius: var(--r-md);
    border: 1px solid var(--line);
    background: var(--bg-3);
    margin-bottom: var(--s-2);
    :deep(.user-chip) {
      flex: 1;
    }
  }
}
</style>
