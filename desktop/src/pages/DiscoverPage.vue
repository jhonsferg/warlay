<script setup lang="ts">
import { Compass, Plus, Search } from "@lucide/vue";
import { onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { errorMessage } from "../services/http";
import type { CommunitySummary } from "../services/types";
import { useCommunities } from "../stores/communities";
import { useDialogs } from "../stores/dialogs";
import { useToasts } from "../stores/toasts";
import WButton from "../ui/atoms/WButton.vue";
import WInput from "../ui/atoms/WInput.vue";
import CommunityTile from "../ui/molecules/CommunityTile.vue";
import EmptyState from "../ui/molecules/EmptyState.vue";
import SectionHeader from "../ui/molecules/SectionHeader.vue";
import HomeSidebar from "../ui/organisms/HomeSidebar.vue";
import AppShell from "../ui/templates/AppShell.vue";

const communities = useCommunities();
const dialogs = useDialogs();
const toasts = useToasts();
const router = useRouter();
const query = ref("");
const joining = ref<string | null>(null);
let timer: ReturnType<typeof setTimeout> | null = null;

watch(query, (q) => {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => void communities.discover(q.trim()), 250);
});
onMounted(() => void communities.discover());

async function join(c: CommunitySummary) {
  joining.value = c.id;
  try {
    await communities.join(c.id);
    toasts.success(`Te uniste a ${c.name}`);
    void router.push(`/c/${c.id}`);
  } catch (e) {
    toasts.error("No se pudo unir", errorMessage(e));
  } finally {
    joining.value = null;
  }
}
</script>

<template>
  <AppShell title="Explorar comunidades" subtitle="Clanes, ligas y grupos de juego">
    <template #sidebar><HomeSidebar /></template>
    <template #title-icon><Compass :size="20" /></template>
    <template #actions>
      <WButton size="sm" @click="dialogs.createCommunity = true"
        ><template #icon><Plus /></template>Crear comunidad</WButton
      >
    </template>

    <section class="discover__hero">
      <h2>Encuentra tu gente</h2>
      <p>Únete a comunidades públicas para chatear, organizar partidas y conocer jugadores.</p>
      <WInput
        v-model="query"
        size="lg"
        placeholder="Buscar comunidades..."
        aria-label="Buscar comunidades"
        class="discover__search"
      >
        <template #icon><Search /></template>
      </WInput>
    </section>

    <SectionHeader :title="query ? 'Resultados' : 'Populares'" :count="communities.discovered.length" />
    <div v-if="communities.discovered.length" class="discover__grid">
      <CommunityTile
        v-for="c in communities.discovered"
        :key="c.id"
        :community="c"
        :busy="joining === c.id"
        @join="join"
        @open="router.push(`/c/${$event.id}`)"
      />
    </div>
    <EmptyState v-else title="No encontramos comunidades" text="Crea la primera y reúne a tus amigos.">
      <template #icon><Compass /></template>
      <WButton @click="dialogs.createCommunity = true"
        ><template #icon><Plus /></template>Crear comunidad</WButton
      >
    </EmptyState>
  </AppShell>
</template>

<style scoped lang="scss">
.discover {
  &__hero {
    text-align: center;
    padding: var(--s-10) var(--s-6);
    margin-bottom: var(--s-8);
    border-radius: var(--r-xl);
    border: 1px solid var(--line-2);
    background: radial-gradient(500px 240px at 50% 0%, rgba(123, 97, 255, 0.28), transparent 70%),
      linear-gradient(180deg, var(--bg-3), var(--bg-2));
    h2 {
      @include display-font(40px);
      margin: 0;
    }
    p {
      color: var(--text-2);
      margin: var(--s-2) 0 var(--s-6);
    }
  }
  &__search {
    max-width: 520px;
    margin: 0 auto;
  }
  &__grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: var(--s-4);
  }
}
</style>
