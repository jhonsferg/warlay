<script setup lang="ts">
import { Check, Search, UserPlus, Users, X } from "@lucide/vue";
import { computed, ref } from "vue";
import { errorMessage } from "../../services/http";
import { matchesQuery } from "../../services/friendOrder";
import { useDialogs } from "../../stores/dialogs";
import { useSocial } from "../../stores/social";
import { useToasts } from "../../stores/toasts";
import WButton from "../atoms/WButton.vue";
import WIconButton from "../atoms/WIconButton.vue";
import WInput from "../atoms/WInput.vue";
import EmptyState from "../molecules/EmptyState.vue";
import UserChip from "../molecules/UserChip.vue";
import { t } from "../../services/i18n";

/**
 * The Messenger-style left column: every friend (connected or not), a search
 * box and, on top, the friend requests that need an answer. Clicking a friend
 * selects them; the page opens the conversation.
 */
defineProps<{ selectedId?: string }>();
const emit = defineEmits<{ select: [id: string] }>();

const social = useSocial();
const dialogs = useDialogs();
const toasts = useToasts();
const query = ref("");

const shown = computed(() => social.sortedFriends.filter((f) => matchesQuery(f, query.value)));

async function answer(fn: () => Promise<unknown>, failure: string) {
  try {
    await fn();
  } catch (e) {
    toasts.error(failure, errorMessage(e));
  }
}
</script>

<template>
  <section class="friends-list" :aria-label="t('Lista de amigos')">
    <div class="friends-list__search">
      <WInput
        v-model="query"
        :placeholder="t('Buscar amigos')"
        data-testid="friends-search"
        :aria-label="t('Buscar amigos')"
      >
        <template #icon><Search /></template>
      </WInput>
    </div>

    <div class="friends-list__scroll">
      <section
        v-if="social.incoming.length || social.outgoing.length"
        class="friends-list__requests"
        data-testid="friend-requests"
      >
        <p class="friends-list__eyebrow">
          {{ t("Solicitudes de amistad — {n}", { n: social.incoming.length + social.outgoing.length }) }}
        </p>
        <div v-for="r in social.incoming" :key="r.id" class="friends-list__request" :data-request="r.from.username">
          <UserChip
            :user="r.from"
            :subtitle="t('@{username} quiere ser tu amigo', { username: r.from.username })"
            size="sm"
          />
          <WButton size="sm" variant="success" @click="answer(() => social.accept(r.id), t('No se pudo aceptar'))"
            ><template #icon><Check /></template>{{ t("Aceptar") }}</WButton
          >
          <WIconButton
            :label="t('Rechazar')"
            size="sm"
            tone="danger"
            @click="answer(() => social.decline(r.id), t('No se pudo rechazar'))"
            ><X
          /></WIconButton>
        </div>
        <div v-for="r in social.outgoing" :key="r.id" class="friends-list__request">
          <UserChip
            :user="r.to"
            :subtitle="t('@{username} · esperando respuesta', { username: r.to.username })"
            size="sm"
          />
          <WButton size="sm" variant="ghost" @click="answer(() => social.decline(r.id), t('No se pudo cancelar'))">{{
            t("Cancelar")
          }}</WButton>
        </div>
      </section>

      <p class="friends-list__eyebrow">{{ t("Todos los amigos — {n}", { n: social.friends.length }) }}</p>

      <EmptyState
        v-if="social.friends.length === 0"
        :title="t('Aún no tienes amigos')"
        :text="t('Busca a tus compañeros por su nombre de usuario.')"
      >
        <template #icon><Users /></template>
        <WButton @click="dialogs.addFriend = true"
          ><template #icon><UserPlus /></template>{{ t("Agregar amigo") }}</WButton
        >
      </EmptyState>
      <p v-else-if="shown.length === 0" class="friends-list__none" data-testid="friends-no-match">
        {{ t("Ningún amigo coincide con «{query}».", { query }) }}
      </p>

      <button
        v-for="f in shown"
        :key="f.id"
        type="button"
        class="friends-list__row"
        :class="{ 'friends-list__row--selected': f.id === selectedId }"
        :data-friend="f.username"
        :aria-current="f.id === selectedId ? 'true' : undefined"
        @click="emit('select', f.id)"
      >
        <UserChip :user="f" :presence="f.presence" />
      </button>
    </div>
  </section>
</template>

<style scoped lang="scss">
.friends-list {
  display: flex;
  flex-direction: column;
  min-height: 0;
  height: 100%;
  --dot-ring: var(--bg-2);

  &__search {
    padding: var(--s-3) var(--s-3) var(--s-2);
  }
  &__scroll {
    flex: 1;
    overflow-y: auto;
    padding: 0 var(--s-2) var(--s-3);
  }
  &__eyebrow {
    @include eyebrow;
    margin: var(--s-3) var(--s-2) var(--s-1);
  }
  &__none {
    margin: var(--s-4) var(--s-2);
    color: var(--text-3);
    font-size: var(--fs-sm);
  }
  &__requests {
    margin-bottom: var(--s-2);
  }
  &__request {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: var(--s-2);
    border-radius: var(--r-md);
    background: var(--bg-3);
    margin-bottom: var(--s-1);
    :deep(.user-chip) {
      flex: 1;
    }
  }
  &__row {
    @include interactive;
    display: block;
    width: 100%;
    padding: var(--s-2) var(--s-3);
    text-align: left;
    background: transparent;
    border: 0;
    border-radius: var(--r-md);
    color: inherit;

    &:hover {
      background: var(--bg-4);
    }
    &--selected,
    &--selected:hover {
      background: var(--accent-soft);
    }
  }
}
</style>
