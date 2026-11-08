<script setup lang="ts">
import { CalendarDays, UserMinus, X } from "@lucide/vue";
import { computed, ref, watch } from "vue";
import { users } from "../../services/api";
import { lastSeenText, shortDate } from "../../services/lastSeen";
import type { Profile, UserWithPresence } from "../../services/types";
import WAvatar from "../atoms/WAvatar.vue";
import WButton from "../atoms/WButton.vue";
import WIconButton from "../atoms/WIconButton.vue";
import { useNow } from "../useNow";

/**
 * A friend's profile. Removing is only *requested* here: the page confirms and
 * runs it, because this panel unmounts the moment the friend disappears and a
 * Vue component cannot emit once unmounted.
 */
const props = defineProps<{ friend: UserWithPresence }>();
const emit = defineEmits<{ close: []; remove: [] }>();

const now = useNow();

const profile = ref<Profile | null>(null);
const failed = ref(false);

watch(
  () => props.friend.id,
  async (id) => {
    profile.value = null;
    failed.value = false;
    try {
      const loaded = await users.profile(id);
      if (id === props.friend.id) profile.value = loaded;
    } catch {
      failed.value = true;
    }
  },
  { immediate: true },
);

const status = computed(() => lastSeenText(props.friend.presence, now.value));
const since = computed(() => (profile.value ? shortDate(new Date(profile.value.memberSince)) : ""));
</script>

<template>
  <aside class="profile" aria-label="Perfil" data-testid="friend-profile">
    <header class="profile__head">
      <span>Perfil</span>
      <WIconButton label="Cerrar perfil" size="sm" data-testid="profile-close" @click="emit('close')"><X /></WIconButton>
    </header>

    <div class="profile__card">
      <WAvatar :name="friend.displayName" :color="friend.avatarColor" size="xl" :status="friend.presence.status" />
      <h3 data-testid="profile-name">{{ friend.displayName }}</h3>
      <p class="profile__username">@{{ friend.username }}</p>
      <p class="profile__status" data-testid="profile-status">{{ status }}</p>
    </div>

    <dl class="profile__details">
      <div>
        <dt>Biografía</dt>
        <dd v-if="profile" data-testid="profile-bio">{{ profile.bio || "Sin biografía." }}</dd>
        <dd v-else-if="failed" class="profile__muted">No se pudo cargar el perfil.</dd>
        <dd v-else class="profile__muted">Cargando…</dd>
      </div>
      <div v-if="profile">
        <dt><CalendarDays :size="14" /> Miembro desde</dt>
        <dd data-testid="profile-since">{{ since }}</dd>
      </div>
    </dl>

    <div class="profile__actions">
      <WButton variant="danger" block data-testid="profile-remove" @click="emit('remove')">
        <template #icon><UserMinus /></template>Eliminar de mis amigos
      </WButton>
    </div>
  </aside>
</template>

<style scoped lang="scss">
.profile {
  width: 300px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: var(--s-4);
  padding: var(--s-4);
  background: var(--bg-2);
  border-left: 1px solid var(--line);
  overflow-y: auto;

  &__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    @include eyebrow;
  }
  &__card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--s-1);
    text-align: center;
    --dot-ring: var(--bg-2);

    h3 {
      margin: var(--s-2) 0 0;
      @include display-font(var(--fs-xl));
    }
  }
  &__username,
  &__status,
  &__muted {
    margin: 0;
    color: var(--text-3);
    font-size: var(--fs-sm);
  }
  &__details {
    margin: 0;
    display: grid;
    gap: var(--s-3);

    dt {
      display: flex;
      align-items: center;
      gap: var(--s-1);
      @include eyebrow;
      margin-bottom: 2px;
    }
    dd {
      margin: 0;
      color: var(--text-2);
      overflow-wrap: anywhere;
    }
  }
  &__actions {
    margin-top: auto;
  }
}
</style>
