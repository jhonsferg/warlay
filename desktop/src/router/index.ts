import { createRouter, createWebHashHistory } from "vue-router";
import { useSession } from "../stores/session";

// Hash history: the desktop webview serves files, not a server with routes.
export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/login", component: () => import("../pages/LoginPage.vue"), meta: { guest: true } },
    { path: "/register", component: () => import("../pages/RegisterPage.vue"), meta: { guest: true } },
    { path: "/", component: () => import("../pages/LobbyPage.vue") },
    { path: "/friends", component: () => import("../pages/FriendsPage.vue") },
    { path: "/dm/:id", component: () => import("../pages/DirectMessagePage.vue") },
    { path: "/discover", component: () => import("../pages/DiscoverPage.vue") },
    { path: "/c/:id", component: () => import("../pages/CommunityPage.vue") },
    { path: "/room/:id", component: () => import("../pages/RoomPage.vue") },
    { path: "/settings", component: () => import("../pages/SettingsPage.vue") },
    { path: "/:pathMatch(.*)*", redirect: "/" },
  ],
});

router.beforeEach(async (to) => {
  const session = useSession();
  if (session.status === "unknown") await session.restore();
  const signedIn = session.status === "signedIn";
  if (to.meta.guest && signedIn) return "/";
  if (!to.meta.guest && !signedIn) return { path: "/login", query: to.path === "/" ? {} : { next: to.fullPath } };
  return true;
});
