<script setup lang="ts">
import {
  Crown,
  DoorOpen,
  Gamepad2,
  Play,
  Radio,
  Repeat2,
  ShieldCheck,
  UserMinus,
  UserPlus,
  Waypoints,
  Wifi,
} from "@lucide/vue";
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { errorMessage } from "../services/http";
import type { UserSummary } from "../services/types";
import { useRooms } from "../stores/rooms";
import { useSession } from "../stores/session";
import { useSettings } from "../stores/settings";
import { useSocial } from "../stores/social";
import { useToasts } from "../stores/toasts";
import WAvatar from "../ui/atoms/WAvatar.vue";
import WBadge from "../ui/atoms/WBadge.vue";
import WButton from "../ui/atoms/WButton.vue";
import WIconButton from "../ui/atoms/WIconButton.vue";
import EmptyState from "../ui/molecules/EmptyState.vue";
import ChatPanel from "../ui/organisms/ChatPanel.vue";
import HomeSidebar from "../ui/organisms/HomeSidebar.vue";
import InviteFriendsDialog from "../ui/organisms/InviteFriendsDialog.vue";
import AppShell from "../ui/templates/AppShell.vue";

const route = useRoute();
const router = useRouter();
const rooms = useRooms();
const session = useSession();
const settings = useSettings();
const social = useSocial();
const toasts = useToasts();
const inviting = ref(false);
const launching = ref(false);

const room = computed(() => (rooms.current?.id === route.params.id ? rooms.current : null));
const emptySlots = computed(() => (room.value ? Math.max(0, room.value.maxPlayers - room.value.members.length) : 0));

watch(
  () => rooms.current,
  (current) => {
    if (!current) void router.replace("/");
  },
);

type Link = { label: string; tone: "success" | "info" | "warning" | "danger" | "neutral"; icon: typeof Wifi };
function linkOf(member: UserSummary): Link | null {
  if (member.id === session.me?.id) return null;
  const link = rooms.linkOf(member.id);
  switch (link?.state) {
    case "connected":
      return { label: "P2P directo", tone: "success", icon: Wifi };
    case "relayed":
      return { label: "Vía ayudante", tone: "info", icon: Waypoints };
    case "failed":
      return { label: "Reintentando", tone: "danger", icon: Repeat2 };
    case "connecting":
    case "not_attempted":
      return { label: "Conectando", tone: "warning", icon: Radio };
    default:
      return { label: "Esperando", tone: "neutral", icon: Radio };
  }
}

const directCount = computed(() => rooms.links.filter((l) => l.state === "connected").length);
const relayCount = computed(() => rooms.links.filter((l) => l.state === "relayed").length);

async function play() {
  launching.value = true;
  try {
    await settings.load();
    await rooms.play(settings.gameExecutablePath, settings.gameArgs);
    toasts.success(
      settings.gameExecutablePath ? "Lanzando Warcraft III" : "Túnel de juego listo",
      settings.gameExecutablePath ? undefined : "Configura el ejecutable en Ajustes para lanzarlo desde aquí.",
    );
  } catch (e) {
    toasts.error("No se pudo iniciar la partida", errorMessage(e));
  } finally {
    launching.value = false;
  }
}

async function leave() {
  try {
    if (rooms.isHost) await rooms.close();
    else await rooms.leave();
    void router.push("/");
  } catch (e) {
    toasts.error("No se pudo salir", errorMessage(e));
  }
}

const isFriend = (id: string) => !!social.friend(id);
const isPending = (id: string) => social.outgoing.some((r) => r.to.id === id);

async function addFriend(member: UserSummary) {
  try {
    const status = await social.sendRequestById(member.id);
    toasts.success(
      status === "accepted" ? `Ahora eres amigo de ${member.displayName}` : "Solicitud enviada",
      `@${member.username}`,
    );
  } catch (e) {
    toasts.error("No se pudo enviar la solicitud", errorMessage(e));
  }
}

async function kick(member: UserSummary) {
  try {
    await rooms.kick(member.id);
  } catch (e) {
    toasts.error("No se pudo expulsar", errorMessage(e));
  }
}
</script>

<template>
  <AppShell :title="room?.name ?? 'Sala'" :subtitle="room?.communityName ?? 'Sala pública'" flush>
    <template #sidebar><HomeSidebar /></template>
    <template #title-icon><Gamepad2 :size="20" /></template>
    <template #actions>
      <WButton v-if="room" size="sm" variant="secondary" @click="inviting = true"
        ><template #icon><UserPlus /></template>Invitar</WButton
      >
    </template>

    <EmptyState v-if="!room" title="No estás en esta sala" text="Únete desde la lista de salas.">
      <template #icon><Gamepad2 /></template>
      <WButton @click="router.push('/')">Ver salas</WButton>
    </EmptyState>

    <div v-else class="room">
      <div class="room__main">
        <section class="room__banner" :class="{ 'room__banner--ingame': room.status === 'in_game' }">
          <div>
            <div class="room__badges">
              <WBadge v-if="room.status === 'in_game'" tone="game" dot>En partida</WBadge>
              <WBadge v-else tone="success" dot>Esperando jugadores</WBadge>
              <WBadge tone="neutral">{{ room.members.length }}/{{ room.maxPlayers }} jugadores</WBadge>
              <WBadge v-if="room.hostAway" tone="danger" dot data-testid="host-away">Anfitrión desconectado</WBadge>
              <WBadge v-if="rooms.isHost" tone="warning"><Crown /> Eres el anfitrión</WBadge>
            </div>
            <h2>{{ room.name }}</h2>
            <p class="room__p2p">
              <ShieldCheck :size="15" />
              <template v-if="room.members.length < 2"
                >Cuando entren jugadores se conectarán directamente contigo.</template
              >
              <template v-else>
                {{ directCount }} enlace{{ directCount === 1 ? "" : "s" }} directo{{ directCount === 1 ? "" : "s"
                }}<template v-if="relayCount"> · {{ relayCount }} vía ayudante</template> · servidor:
                {{ rooms.session?.helper.name }} ({{ rooms.session?.helper.region }})
              </template>
            </p>
          </div>
          <div class="room__cta">
            <WButton size="lg" variant="success" :loading="launching" data-testid="play" @click="play">
              <template #icon><Play /></template>{{ rooms.tunnelActive ? "Abrir el juego" : "Jugar" }}
            </WButton>
            <WButton size="lg" variant="danger" data-testid="leave-room" @click="leave">
              <template #icon><DoorOpen /></template>{{ rooms.isHost ? "Cerrar sala" : "Salir" }}
            </WButton>
          </div>
        </section>

        <section class="room__roster" aria-label="Jugadores">
          <article v-for="m in room.members" :key="m.id" class="slot" :data-member="m.username">
            <WAvatar :name="m.displayName" :color="m.avatarColor" size="lg" />
            <div class="slot__info">
              <strong>
                {{ m.displayName }}
                <Crown v-if="m.id === room.host.id" class="slot__crown" :size="14" />
              </strong>
              <span v-if="m.id === session.me?.id" class="slot__you">Tú</span>
              <WBadge v-else-if="linkOf(m)" :tone="linkOf(m)!.tone"
                ><component :is="linkOf(m)!.icon" />{{ linkOf(m)!.label }}</WBadge
              >
            </div>
            <WIconButton
              v-if="m.id !== session.me?.id && !isFriend(m.id) && !isPending(m.id)"
              label="Añadir amigo"
              size="sm"
              @click="addFriend(m)"
              ><UserPlus
            /></WIconButton>
            <WBadge v-else-if="m.id !== session.me?.id && isPending(m.id)" tone="neutral">Solicitud enviada</WBadge>
            <WIconButton
              v-if="rooms.isHost && m.id !== session.me?.id"
              label="Expulsar"
              size="sm"
              tone="danger"
              @click="kick(m)"
              ><UserMinus
            /></WIconButton>
          </article>
          <article v-for="n in emptySlots" :key="`empty-${n}`" class="slot slot--empty">
            <span class="slot__ghost" />
            <span>Libre</span>
          </article>
        </section>
      </div>

      <aside class="room__chat">
        <header class="room__chat-head">Chat de la sala</header>
        <ChatPanel
          kind="room"
          :target-id="room.id"
          placeholder="Escribe a la sala..."
          empty-title="Saluda a tus compañeros"
        />
      </aside>
    </div>

    <InviteFriendsDialog
      v-if="inviting && room"
      title="Invitar a la sala"
      :exclude="room.members.map((m) => m.id)"
      :invite="rooms.invite"
      @close="inviting = false"
    />
  </AppShell>
</template>

<style scoped lang="scss">
.room {
  display: flex;
  flex: 1;
  min-height: 0;

  &__main {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: var(--s-8);
    container-type: inline-size;
  }

  &__banner {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--s-6);
    padding: var(--s-6) var(--s-8);
    margin-bottom: var(--s-6);
    border-radius: var(--r-xl);
    border: 1px solid rgba(52, 211, 153, 0.25);
    background: radial-gradient(500px 220px at 100% 100%, rgba(52, 211, 153, 0.16), transparent 60%),
      linear-gradient(135deg, var(--bg-3), var(--bg-2));

    &--ingame {
      border-color: rgba(244, 114, 182, 0.35);
      background: radial-gradient(500px 220px at 100% 100%, rgba(244, 114, 182, 0.18), transparent 60%),
        linear-gradient(135deg, var(--bg-3), var(--bg-2));
    }

    h2 {
      @include display-font(36px);
      margin: var(--s-3) 0 var(--s-2);
    }

    // Narrow (both side panels open): stack the actions under the title.
    @container (max-width: 680px) {
      flex-direction: column;
      align-items: stretch;
      padding: var(--s-6);
    }
  }
  &__badges {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  &__p2p {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    svg {
      flex-shrink: 0;
      margin-top: 2px;
    }
    margin: 0;
    color: var(--text-2);
    svg {
      color: var(--success);
    }
  }
  &__cta {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    min-width: 190px;

    @container (max-width: 680px) {
      flex-direction: row;
      > * {
        flex: 1;
      }
    }
  }

  &__roster {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: var(--s-3);
  }

  &__chat {
    width: 330px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--line);
    background: var(--bg-2);
  }
  &__chat-head {
    @include eyebrow;
    padding: var(--s-4) var(--s-5);
    border-bottom: 1px solid var(--line);
  }
}

.slot {
  @include card(var(--s-4));
  display: flex;
  align-items: center;
  gap: var(--s-3);

  &__info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    strong {
      @include truncate;
      display: flex;
      align-items: center;
      gap: 6px;
      max-width: 100%;
    }
  }
  &__crown {
    color: var(--in-room);
    flex-shrink: 0;
  }
  &__you {
    font-size: var(--fs-sm);
    color: var(--text-3);
  }

  &--empty {
    border-style: dashed;
    background: transparent;
    color: var(--text-3);
    font-size: var(--fs-sm);
  }
  &__ghost {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    border: 1.5px dashed var(--line-3);
  }
}
</style>
