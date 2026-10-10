<script setup lang="ts">
import { Compass, Plus } from "@lucide/vue";
import { useRoute, useRouter } from "vue-router";
import { useCommunities } from "../../stores/communities";
import WAvatar from "../atoms/WAvatar.vue";
import WLogo from "../atoms/WLogo.vue";

defineEmits<{ createCommunity: [] }>();
const communities = useCommunities();
const route = useRoute();
const router = useRouter();

const isHome = () => !route.path.startsWith("/c/") && route.path !== "/discover";
</script>

<template>
  <nav class="app-rail" aria-label="Comunidades">
    <button
      type="button"
      class="app-rail__item"
      :class="{ 'app-rail__item--active': isHome() }"
      title="Inicio"
      aria-label="Inicio"
      @click="router.push('/')"
    >
      <WLogo :size="44" />
    </button>
    <div class="app-rail__divider" />
    <div class="app-rail__list">
      <button
        v-for="c in communities.mine"
        :key="c.id"
        type="button"
        class="app-rail__item"
        :class="{ 'app-rail__item--active': route.params.id === c.id }"
        :title="c.name"
        :aria-label="c.name"
        @click="router.push(`/c/${c.id}`)"
      >
        <WAvatar :name="c.name" :color="c.accentColor" size="lg" shape="squircle" />
      </button>
    </div>
    <button
      type="button"
      class="app-rail__action"
      title="Crear comunidad"
      aria-label="Crear comunidad"
      @click="$emit('createCommunity')"
    >
      <Plus />
    </button>
    <button
      type="button"
      class="app-rail__action"
      :class="{ 'app-rail__action--active': route.path === '/discover' }"
      title="Explorar comunidades"
      aria-label="Explorar comunidades"
      @click="router.push('/discover')"
    >
      <Compass />
    </button>
  </nav>
</template>

<style scoped lang="scss">
.app-rail {
  width: var(--rail-w);
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--s-2);
  padding: var(--s-3) 0;
  background: var(--bg-0);
  border-right: 1px solid var(--line);

  &__divider {
    width: 32px;
    height: 2px;
    border-radius: 2px;
    background: var(--line-2);
    margin: var(--s-1) 0;
  }

  &__list {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    overflow-y: auto;
    scrollbar-width: none;
  }

  &__item {
    @include reset-button;
    @include interactive;
    position: relative;
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    border-radius: 16px;

    // The pill on the left marks "you are here" (Gestalt: figure/ground).
    &::before {
      content: "";
      position: absolute;
      left: -12px;
      width: 4px;
      height: 0;
      border-radius: 0 4px 4px 0;
      background: var(--text-1);
      transition: height var(--t-med) var(--ease);
    }
    &:hover::before {
      height: 18px;
    }
    &--active::before {
      height: 36px;
    }
    &:hover {
      transform: translateY(-1px);
    }
  }

  &__action {
    @include reset-button;
    @include interactive;
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    border-radius: 16px;
    background: var(--bg-3);
    color: var(--online);

    svg {
      width: 22px;
      height: 22px;
    }
    &:hover,
    &--active {
      background: var(--online);
      color: #052e1f;
      border-radius: 14px;
    }
  }
}
</style>
