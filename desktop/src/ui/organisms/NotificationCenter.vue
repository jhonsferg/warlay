<script setup lang="ts">
import { Bell, Check, X } from "@lucide/vue";
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { errorMessage } from "../../services/http";
import type { AppNotification, InviteView } from "../../services/types";
import { useCommunities } from "../../stores/communities";
import { useNotifications } from "../../stores/notifications";
import { useRooms } from "../../stores/rooms";
import { useSocial } from "../../stores/social";
import { useToasts } from "../../stores/toasts";
import WAvatar from "../atoms/WAvatar.vue";
import WButton from "../atoms/WButton.vue";
import WIconButton from "../atoms/WIconButton.vue";

const notifications = useNotifications();
const social = useSocial();
const rooms = useRooms();
const communities = useCommunities();
const toasts = useToasts();
const router = useRouter();
const open = ref(false);

const badge = computed(() => notifications.unread + notifications.invites.length + social.incoming.length);

function toggle() {
  open.value = !open.value;
  if (open.value) void notifications.markAllRead();
}

function describe(n: AppNotification): string {
  const p = n.payload as Record<string, { displayName?: string } & Record<string, unknown>>;
  switch (n.kind) {
    case "friend_request":
      return `${p.from?.displayName ?? "Alguien"} quiere ser tu amigo`;
    case "friend_accepted":
      return `${p.user?.displayName ?? "Tu amigo"} aceptó tu solicitud`;
    case "room_invite":
      return `${(p.inviter as { displayName?: string })?.displayName ?? "Alguien"} te invitó a la sala ${String(p.targetName ?? "")}`;
    case "community_invite":
      return `${(p.inviter as { displayName?: string })?.displayName ?? "Alguien"} te invitó a ${String(p.targetName ?? "una comunidad")}`;
    default:
      return "Nueva notificación";
  }
}

async function accept(invite: InviteView) {
  try {
    const result = await notifications.acceptInvite(invite.id);
    if (result.kind === "room") {
      await rooms.adopt(result.session);
      void router.push(`/room/${result.session.room.id}`);
    } else {
      await communities.loadMine();
      void router.push(`/c/${result.communityId}`);
    }
    open.value = false;
  } catch (error) {
    toasts.error("No se pudo aceptar la invitación", errorMessage(error));
  }
}

const ago = (iso: string) => {
  const mins = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
  if (mins < 1) return "ahora";
  if (mins < 60) return `hace ${mins} min`;
  const hours = Math.round(mins / 60);
  return hours < 24 ? `hace ${hours} h` : `hace ${Math.round(hours / 24)} d`;
};
</script>

<template>
  <div class="notif">
    <WIconButton label="Notificaciones" :badge="badge" :active="open" data-testid="notifications-button" @click="toggle"
      ><Bell
    /></WIconButton>
    <div v-if="open" class="notif__backdrop" @click="open = false" />
    <section v-if="open" class="notif__panel" aria-label="Notificaciones">
      <header class="notif__head">
        <h3>Actividad</h3>
      </header>

      <div v-if="notifications.invites.length || social.incoming.length" class="notif__group">
        <p class="notif__eyebrow">Pendientes</p>
        <div v-for="r in social.incoming" :key="r.id" class="notif__item notif__item--action">
          <WAvatar :name="r.from.displayName" :color="r.from.avatarColor" size="sm" />
          <p>
            <strong>{{ r.from.displayName }}</strong> quiere ser tu amigo
          </p>
          <WIconButton label="Aceptar" size="sm" @click="social.accept(r.id)"><Check /></WIconButton>
          <WIconButton label="Rechazar" size="sm" tone="danger" @click="social.decline(r.id)"><X /></WIconButton>
        </div>
        <div
          v-for="i in notifications.invites"
          :key="i.id"
          class="notif__item notif__item--action"
          :data-invite="i.targetName"
        >
          <WAvatar :name="i.inviter.displayName" :color="i.inviter.avatarColor" size="sm" />
          <p>
            <strong>{{ i.inviter.displayName }}</strong>
            te invita {{ i.kind === "room" ? "a la sala" : "a la comunidad" }} <strong>{{ i.targetName }}</strong>
          </p>
          <WButton size="sm" @click="accept(i)">Aceptar</WButton>
          <WIconButton label="Rechazar" size="sm" tone="danger" @click="notifications.declineInvite(i.id)"
            ><X
          /></WIconButton>
        </div>
      </div>

      <div class="notif__group">
        <p class="notif__eyebrow">Recientes</p>
        <p v-if="notifications.items.length === 0" class="notif__empty">Todo tranquilo por aquí.</p>
        <div
          v-for="n in notifications.items.slice(0, 20)"
          :key="n.id"
          class="notif__item"
          :class="{ 'notif__item--unread': !n.read }"
        >
          <span class="notif__pip" />
          <p>{{ describe(n) }}</p>
          <time>{{ ago(n.createdAt) }}</time>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped lang="scss">
.notif {
  position: relative;

  &__backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
  }

  &__panel {
    position: absolute;
    right: 0;
    top: calc(100% + 8px);
    z-index: 31;
    width: 380px;
    max-height: 520px;
    overflow-y: auto;
    background: var(--bg-2);
    border: 1px solid var(--line-2);
    border-radius: var(--r-lg);
    box-shadow: var(--shadow-3);
    animation: notif-in var(--t-med) var(--ease);
  }

  &__head {
    padding: var(--s-4) var(--s-4) var(--s-2);
    h3 {
      margin: 0;
      @include display-font(var(--fs-xl));
    }
  }

  &__group {
    padding: var(--s-2) var(--s-2) var(--s-3);
    border-top: 1px solid var(--line);
  }
  &__eyebrow {
    @include eyebrow;
    margin: var(--s-2) var(--s-2);
  }
  &__empty {
    margin: 0 var(--s-2);
    color: var(--text-3);
    font-size: var(--fs-sm);
  }

  &__item {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: var(--s-2);
    border-radius: var(--r-sm);
    p {
      flex: 1;
      margin: 0;
      font-size: var(--fs-sm);
      color: var(--text-2);
    }
    strong {
      color: var(--text-1);
    }
    time {
      font-size: var(--fs-xs);
      color: var(--text-3);
      white-space: nowrap;
    }
    &:hover {
      background: var(--bg-3);
    }
  }
  &__pip {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    background: transparent;
  }
  &__item--unread &__pip {
    background: var(--accent);
  }
}

@keyframes notif-in {
  from {
    opacity: 0;
    transform: translateY(-6px);
  }
}
</style>
