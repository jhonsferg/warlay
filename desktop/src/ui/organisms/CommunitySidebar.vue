<script setup lang="ts">
import { Hash, LogOut, Swords, Trash2, UserPlus, Users } from "@lucide/vue";
import type { CommunityDetail } from "../../services/types";
import WAvatar from "../atoms/WAvatar.vue";

defineProps<{ community: CommunityDetail; view: string }>();
defineEmits<{ select: [view: string]; invite: []; leave: []; remove: [] }>();
</script>

<template>
  <div class="community-sidebar" :style="{ '--c-accent': community.accentColor }">
    <header class="community-sidebar__head">
      <WAvatar :name="community.name" :color="community.accentColor" size="sm" shape="squircle" />
      <h2>{{ community.name }}</h2>
    </header>
    <div class="community-sidebar__banner" aria-hidden="true" />
    <nav class="community-sidebar__nav">
      <p class="community-sidebar__eyebrow">Canales</p>
      <button type="button" :class="{ active: view === 'chat' }" @click="$emit('select', 'chat')">
        <Hash /> general
      </button>
      <button type="button" :class="{ active: view === 'rooms' }" @click="$emit('select', 'rooms')">
        <Swords /> salas
        <span v-if="community.rooms.length" class="community-sidebar__count">{{ community.rooms.length }}</span>
      </button>
      <button type="button" :class="{ active: view === 'members' }" @click="$emit('select', 'members')">
        <Users /> miembros <span class="community-sidebar__count">{{ community.memberCount }}</span>
      </button>
      <p class="community-sidebar__eyebrow">Comunidad</p>
      <button type="button" @click="$emit('invite')"><UserPlus /> Invitar amigos</button>
      <button v-if="community.myRole === 'owner'" type="button" class="danger" @click="$emit('remove')">
        <Trash2 /> Eliminar comunidad
      </button>
      <button v-else-if="community.myRole" type="button" class="danger" @click="$emit('leave')">
        <LogOut /> Salir de la comunidad
      </button>
    </nav>
  </div>
</template>

<style scoped lang="scss">
.community-sidebar {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;

  &__head {
    height: var(--topbar-h);
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: 0 var(--s-4);
    border-bottom: 1px solid var(--line);
    h2 {
      @include display-font(var(--fs-lg));
      @include truncate;
      margin: 0;
    }
  }
  &__banner {
    height: 4px;
    background: linear-gradient(90deg, var(--c-accent), transparent);
  }
  &__nav {
    flex: 1;
    overflow-y: auto;
    padding: var(--s-2) var(--s-3);
    display: flex;
    flex-direction: column;
    gap: 2px;

    button {
      @include reset-button;
      @include interactive;
      display: flex;
      align-items: center;
      gap: var(--s-2);
      height: 36px;
      padding: 0 var(--s-3);
      border-radius: var(--r-sm);
      color: var(--text-2);
      font-weight: 500;
      text-align: left;
      svg {
        width: 17px;
        height: 17px;
        color: var(--text-3);
      }
      &:hover {
        background: var(--bg-4);
        color: var(--text-1);
      }
      &.active {
        background: var(--bg-5);
        color: var(--text-1);
      }
      &.danger:hover {
        background: var(--danger-soft);
        color: var(--danger);
        svg {
          color: var(--danger);
        }
      }
    }
  }
  &__eyebrow {
    @include eyebrow;
    margin: var(--s-4) var(--s-2) var(--s-1);
  }
  &__count {
    margin-left: auto;
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
}
</style>
