<script setup lang="ts">
import { watch } from "vue";
import { useRouter } from "vue-router";
import { isDesktop } from "./services/native";
import { installAppLifecycle } from "./stores/app";
import { useSession } from "./stores/session";
import { useCrashReports } from "./stores/crashReports";
import { announceUpdateOnStart } from "./stores/updates";
import ConfirmDialog from "./ui/organisms/ConfirmDialog.vue";
import CrashReportDialog from "./ui/organisms/CrashReportDialog.vue";
import ReportDialog from "./ui/organisms/ReportDialog.vue";
import ToastHost from "./ui/organisms/ToastHost.vue";

const session = useSession();
const router = useRouter();

installAppLifecycle();

// Notices (a new version, requests, messages) are for someone who is signed
// in: the login screen shows none. The new-version notice is offered once per
// run, the first time a session is up.
let announcedUpdate = false;
watch(
  () => session.status,
  (status) => {
    if (status !== "signedIn" || announcedUpdate || !isDesktop) return;
    announcedUpdate = true;
    void announceUpdateOnStart(() => router.push({ path: "/settings", query: { tab: "advanced" } }));
  },
  { immediate: true },
);

// A crash an earlier run left is offered once there is a session, never on the
// login screen.
const crashes = useCrashReports();
watch(
  () => session.status,
  (status) => {
    if (status === "signedIn" && isDesktop) void crashes.load().catch(() => {});
  },
  { immediate: true },
);

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
  <ReportDialog v-if="session.status === 'signedIn'" />
  <CrashReportDialog v-if="session.status === 'signedIn'" />
  <ToastHost v-if="session.status === 'signedIn'" />
</template>
