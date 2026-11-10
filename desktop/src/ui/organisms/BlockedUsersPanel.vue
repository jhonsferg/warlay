<script setup lang="ts">
import { Ban } from "@lucide/vue";
import { onMounted, ref } from "vue";
import { errorMessage } from "../../services/http";
import { useBlocks } from "../../stores/blocks";
import { useToasts } from "../../stores/toasts";
import WAvatar from "../atoms/WAvatar.vue";
import WButton from "../atoms/WButton.vue";

/** The players the user blocked, with a way to undo it. */
const blocks = useBlocks();
const toasts = useToasts();
const busy = ref<string | null>(null);

onMounted(() => {
  void blocks.load().catch(() => {});
});

async function unblock(id: string, name: string) {
  busy.value = id;
  try {
    await blocks.unblock(id);
    toasts.success("Usuario desbloqueado", name);
  } catch (e) {
    toasts.error("No se pudo desbloquear", errorMessage(e));
  } finally {
    busy.value = null;
  }
}
</script>

<template>
  <section class="blocked" data-testid="blocked-card">
    <h3><Ban /> Usuarios bloqueados</h3>
    <p class="blocked__muted">
      Una persona bloqueada no puede enviarte solicitudes, invitaciones ni mensajes, y no ves sus mensajes en salas y
      comunidades. No se le avisa.
    </p>
    <p v-if="!blocks.list.length" class="blocked__muted" data-testid="blocked-empty">No has bloqueado a nadie.</p>
    <ul v-else class="blocked__list">
      <li v-for="b in blocks.list" :key="b.id" data-testid="blocked-row">
        <WAvatar :name="b.displayName" :color="b.avatarColor" size="sm" />
        <span class="blocked__name"
          >{{ b.displayName }} <small>@{{ b.username }}</small></span
        >
        <WButton size="sm" variant="secondary" :loading="busy === b.id" @click="unblock(b.id, b.displayName)">
          Desbloquear
        </WButton>
      </li>
    </ul>
  </section>
</template>

<style scoped lang="scss">
.blocked {
  @include card(var(--s-6));
  display: flex;
  flex-direction: column;
  gap: var(--s-4);

  h3 {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    margin: 0;
    @include display-font(var(--fs-xl));
    svg {
      width: 20px;
      height: 20px;
      color: var(--accent);
    }
  }

  &__muted {
    margin: 0;
    color: var(--text-3);
  }

  &__list {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    margin: 0;
    padding: 0;
    list-style: none;

    li {
      display: flex;
      align-items: center;
      gap: var(--s-3);
    }
  }

  &__name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    small {
      color: var(--text-3);
    }
  }
}
</style>
