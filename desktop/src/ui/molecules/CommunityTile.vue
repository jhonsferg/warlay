<script setup lang="ts">
import { Globe, Lock, Users } from "@lucide/vue";
import type { CommunitySummary } from "../../services/types";
import WAvatar from "../atoms/WAvatar.vue";
import WButton from "../atoms/WButton.vue";

defineProps<{ community: CommunitySummary; busy?: boolean }>();
defineEmits<{ join: [CommunitySummary]; open: [CommunitySummary] }>();
</script>

<template>
  <article class="community-tile" :style="{ '--tile-accent': community.accentColor }" :data-community="community.name">
    <div class="community-tile__banner" aria-hidden="true" />
    <WAvatar
      class="community-tile__avatar"
      :name="community.name"
      :color="community.accentColor"
      size="lg"
      shape="squircle"
    />
    <div class="community-tile__body">
      <h3>{{ community.name }}</h3>
      <p>{{ community.description || "Sin descripción todavía." }}</p>
      <div class="community-tile__meta">
        <span><Users :size="13" /> {{ community.memberCount }} miembros</span>
        <span v-if="community.isPublic"><Globe :size="13" /> Pública</span>
        <span v-else><Lock :size="13" /> Privada</span>
      </div>
    </div>
    <div class="community-tile__actions">
      <WButton v-if="community.myRole" variant="secondary" size="sm" block @click="$emit('open', community)"
        >Entrar</WButton
      >
      <WButton v-else size="sm" block :loading="busy" @click="$emit('join', community)">Unirme</WButton>
    </div>
  </article>
</template>

<style scoped lang="scss">
.community-tile {
  @include card(0);
  @include interactive;
  overflow: hidden;
  display: flex;
  flex-direction: column;

  &:hover {
    transform: translateY(-2px);
    border-color: var(--line-3);
    box-shadow: var(--shadow-2);
  }

  &__banner {
    height: 72px;
    background: radial-gradient(120% 140% at 0% 0%, var(--tile-accent), transparent 70%),
      linear-gradient(135deg, var(--bg-4), var(--bg-2));
    opacity: 0.9;
  }
  &__avatar {
    margin: -26px 0 0 var(--s-5);
    box-shadow: 0 0 0 4px var(--bg-3);
    border-radius: 30%;
  }
  &__body {
    padding: var(--s-3) var(--s-5) 0;
    flex: 1;
    h3 {
      margin: 0;
      @include display-font(var(--fs-xl));
    }
    p {
      margin: 6px 0 0;
      color: var(--text-3);
      font-size: var(--fs-sm);
      display: -webkit-box;
      -webkit-line-clamp: 2;
      -webkit-box-orient: vertical;
      overflow: hidden;
      min-height: 2.8em;
    }
  }
  &__meta {
    display: flex;
    gap: var(--s-3);
    margin-top: var(--s-3);
    font-size: var(--fs-sm);
    color: var(--text-2);
    span {
      display: inline-flex;
      align-items: center;
      gap: 4px;
    }
  }
  &__actions {
    padding: var(--s-4) var(--s-5) var(--s-5);
  }
}
</style>
