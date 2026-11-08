<script setup lang="ts">
import { onMounted, watch } from "vue";
import { useRouter } from "vue-router";
import { isDesktop } from "./services/native";
import { installAppLifecycle } from "./stores/app";
import { useSession } from "./stores/session";
import ConfirmDialog from "./ui/organisms/ConfirmDialog.vue";
import ToastHost from "./ui/organisms/ToastHost.vue";

const session = useSession();
const router = useRouter();

installAppLifecycle();

// Signing out anywhere (logout, expired session) lands on the login page.
watch(
  () => session.status,
  (status, previous) => {
    if (status === "signedOut" && previous === "signedIn") void router.replace("/login");
  },
);

onMounted(() => {
  if (!isDesktop) return;
  // The network agent now runs inside this same process, so there is no
  // separate agent to start. The agent is ready as soon as the app opens.
});
</script>

<template>
  <RouterView />
  <ConfirmDialog />
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
