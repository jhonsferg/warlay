<script setup lang="ts">
import { Gamepad2, MessageCircle, Swords, Users } from "@lucide/vue";
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useChat } from "../../stores/chat";
import { useSocial } from "../../stores/social";
import WAvatar from "../atoms/WAvatar.vue";
import { useRooms } from "../../stores/rooms";

const route = useRoute();
const router = useRouter();
const social = useSocial();
const chat = useChat();
const rooms = useRooms();

const dms = computed(() =>
  chat.recentDms
    .map((id) => social.friend(id))
    .filter((f): f is NonNullable<typeof f> => !!f)
    .slice(0, 12),
);

const nav = computed(() => [
  { to: "/", label: "Salas", icon: Swords, badge: 0 },
  { to: "/friends", label: "Amigos", icon: Users, badge: social.incoming.length },
  ...(rooms.current ? [{ to: `/room/${rooms.current.id}`, label: "Mi sala", icon: Gamepad2, badge: 0 }] : []),
]);
</script>

<template>
  <div class="home-sidebar">
    <header class="home-sidebar__head">
      <h2>Inicio</h2>
    </header>
    <nav class="home-sidebar__nav">
      <button
        v-for="item in nav"
        :key="item.to"
        type="button"
        class="home-sidebar__link"
        :class="{ 'home-sidebar__link--active': route.path === item.to }"
        @click="router.push(item.to)"
      >
        <component :is="item.icon" />
        <span>{{ item.label }}</span>
        <span v-if="item.badge" class="home-sidebar__badge">{{ item.badge }}</span>
      </button>
    </nav>

    <p class="home-sidebar__eyebrow"><MessageCircle :size="12" /> Mensajes directos</p>
    <div class="home-sidebar__dms">
      <p v-if="dms.length === 0" class="home-sidebar__hint">Abre un chat desde tu lista de amigos.</p>
      <button
        v-for="f in dms"
        :key="f.id"
        type="button"
        class="home-sidebar__dm"
        :class="{ 'home-sidebar__link--active': route.path === `/dm/${f.id}` }"
        @click="router.push(`/dm/${f.id}`)"
      >
        <WAvatar :name="f.displayName" :color="f.avatarColor" size="sm" :status="f.presence.status" />
        <span class="home-sidebar__dm-name">{{ f.displayName }}</span>
        <span v-if="chat.unread[`dm:${f.id}`]" class="home-sidebar__badge">{{ chat.unread[`dm:${f.id}`] }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped lang="scss">
.home-sidebar {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1;

  &__head {
    height: var(--topbar-h);
    display: flex;
    align-items: center;
    padding: 0 var(--s-5);
    border-bottom: 1px solid var(--line);
    h2 {
      margin: 0;
      @include display-font(var(--fs-xl));
    }
  }

  &__nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--s-3);
  }

  &__link,
  &__dm {
    @include reset-button;
    @include interactive;
    display: flex;
    align-items: center;
    gap: var(--s-3);
    height: 40px;
    padding: 0 var(--s-3);
    border-radius: var(--r-sm);
    color: var(--text-2);
    font-weight: 600;
    text-align: left;

    svg {
      width: 18px;
      height: 18px;
    }
    &:hover {
      background: var(--bg-4);
      color: var(--text-1);
    }
    &--active {
      background: var(--bg-5);
      color: var(--text-1);
    }
  }
  &__dm {
    height: 44px;
    font-weight: 500;
    --dot-ring: var(--bg-2);
  }
  &__dm-name {
    @include truncate;
    flex: 1;
  }

  &__badge {
    margin-left: auto;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: var(--r-full);
    background: var(--danger);
    color: #fff;
    font-size: 10px;
    font-weight: 700;
    line-height: 18px;
    text-align: center;
  }

  &__eyebrow {
    @include eyebrow;
    display: flex;
    align-items: center;
    gap: 6px;
    margin: var(--s-4) var(--s-5) var(--s-2);
  }
  &__dms {
    flex: 1;
    overflow-y: auto;
    padding: 0 var(--s-3) var(--s-3);
  }
  &__hint {
    margin: 0 var(--s-2);
    color: var(--text-3);
    font-size: var(--fs-sm);
  }
}
</style>
