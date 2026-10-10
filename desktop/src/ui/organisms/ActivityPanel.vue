<script setup lang="ts">
import { MessageCircle, UserPlus } from "@lucide/vue";
import { computed } from "vue";
import { useRouter } from "vue-router";
import type { PresenceStatus } from "../../services/types";
import { useRooms } from "../../stores/rooms";
import { useSocial } from "../../stores/social";
import { useToasts } from "../../stores/toasts";
import WIconButton from "../atoms/WIconButton.vue";
import FriendRow from "../molecules/FriendRow.vue";
import { PRESENCE_ORDER, presenceLabel } from "../presence";

const social = useSocial();
const rooms = useRooms();
const toasts = useToasts();
const router = useRouter();

// Proximity + common region: friends grouped by what they're doing.
const groups = computed(() =>
  PRESENCE_ORDER.map((status: PresenceStatus) => ({
    status,
    label: presenceLabel(status),
    friends: social.sortedFriends.filter((f) => f.presence.status === status),
  })).filter((g) => g.friends.length > 0),
);

async function invite(userId: string) {
  try {
    await rooms.invite(userId);
    toasts.success("Invitación enviada");
  } catch (e) {
    toasts.error("No se pudo invitar", e instanceof Error ? e.message : String(e));
  }
}
</script>

<template>
  <aside class="activity" aria-label="Amigos">
    <header class="activity__head">
      <h3>Amigos</h3>
      <span class="activity__online">{{ social.onlineCount }} en línea</span>
    </header>
    <div class="activity__body">
      <p v-if="social.friends.length === 0" class="activity__empty">Agrega amigos para ver aquí qué están jugando.</p>
      <section v-for="g in groups" :key="g.status" class="activity__group">
        <p class="activity__eyebrow" :class="`activity__eyebrow--${g.status}`">
          {{ g.label }} — {{ g.friends.length }}
        </p>
        <FriendRow v-for="f in g.friends" :key="f.id" :friend="f" compact>
          <template #actions>
            <WIconButton label="Mensaje" size="sm" @click="router.push(`/dm/${f.id}`)"><MessageCircle /></WIconButton>
            <WIconButton
              v-if="
                rooms.current && f.presence.status !== 'offline' && !rooms.current.members.some((m) => m.id === f.id)
              "
              label="Invitar a mi sala"
              size="sm"
              @click="invite(f.id)"
              ><UserPlus
            /></WIconButton>
          </template>
        </FriendRow>
      </section>
    </div>
  </aside>
</template>

<style scoped lang="scss">
.activity {
  width: var(--activity-w);
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  background: var(--bg-2);
  border-left: 1px solid var(--line);
  --dot-ring: var(--bg-2);

  &__head {
    height: var(--topbar-h);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--s-5);
    border-bottom: 1px solid var(--line);
    h3 {
      margin: 0;
      @include display-font(var(--fs-lg));
    }
  }
  &__online {
    font-size: var(--fs-sm);
    color: var(--online);
    font-weight: 600;
  }
  &__body {
    flex: 1;
    overflow-y: auto;
    padding: var(--s-2) var(--s-3);
  }
  &__empty {
    color: var(--text-3);
    font-size: var(--fs-sm);
    padding: var(--s-3);
  }
  &__group {
    margin-bottom: var(--s-3);
  }
  &__eyebrow {
    @include eyebrow;
    margin: var(--s-3) var(--s-2) var(--s-1);
    &--in_game {
      color: var(--in-game);
    }
    &--in_room {
      color: var(--in-room);
    }
    &--online {
      color: var(--online);
    }
  }
}
</style>
