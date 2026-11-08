<script setup lang="ts">
import { watch } from "vue";
import { useRouter } from "vue-router";
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
</script>

<template>
  <RouterView />
  <ConfirmDialog />
  <ToastHost />
</template>
