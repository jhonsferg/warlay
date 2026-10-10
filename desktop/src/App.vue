<script setup lang="ts">
import { TriangleAlert } from "@lucide/vue";
import { onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { isDesktop, native } from "./services/native";
import { installAppLifecycle } from "./stores/app";
import { useSession } from "./stores/session";
import ToastHost from "./ui/organisms/ToastHost.vue";

const session = useSession();
const router = useRouter();
const agentError = ref<string | null>(null);

installAppLifecycle();

// Signing out anywhere (logout, expired session) lands on the login page.
watch(
  () => session.status,
  (status, previous) => {
    if (status === "signedOut" && previous === "signedIn") void router.replace("/login");
  },
);

onMounted(async () => {
  if (!isDesktop) return;
  // The agent (daemon) does the networking for games; the social side of
  // the app works without it, so a failure here is a banner, not a wall.
  try {
    await native.ensureDaemonRunning();
  } catch (error) {
    agentError.value = error instanceof Error ? error.message : String(error);
  }
});
</script>

<template>
  <div v-if="agentError" class="agent-banner" role="alert">
    <TriangleAlert :size="16" />
    El agente de red no pudo iniciar: {{ agentError }}. Podrás chatear, pero no jugar hasta reiniciar Warlay.
  </div>
  <RouterView />
  <ToastHost />
</template>

<style scoped lang="scss">
.agent-banner {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  z-index: 70;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--s-2);
  padding: var(--s-2) var(--s-4);
  background: var(--danger);
  color: #fff;
  font-size: var(--fs-sm);
  font-weight: 600;
}
</style>
