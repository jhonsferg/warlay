<script setup lang="ts">
import { useDialogs } from "../../stores/dialogs";
import ActivityPanel from "../organisms/ActivityPanel.vue";
import AddFriendDialog from "../organisms/AddFriendDialog.vue";
import AppRail from "../organisms/AppRail.vue";
import CreateCommunityDialog from "../organisms/CreateCommunityDialog.vue";
import CreateRoomDialog from "../organisms/CreateRoomDialog.vue";
import TopBar from "../organisms/TopBar.vue";
import UserPanel from "../organisms/UserPanel.vue";

defineProps<{ title: string; subtitle?: string; flush?: boolean }>();
const dialogs = useDialogs();
</script>

<template>
  <div class="app-shell">
    <AppRail @create-community="dialogs.createCommunity = true" />
    <aside class="app-shell__sidebar">
      <slot name="sidebar" />
      <UserPanel />
    </aside>
    <main class="app-shell__main">
      <TopBar
        :title="title"
        :subtitle="subtitle"
        :activity-open="dialogs.activityOpen"
        @toggle-activity="dialogs.toggleActivity()"
      >
        <template #icon><slot name="title-icon" /></template>
        <slot name="actions" />
      </TopBar>
      <div class="app-shell__content" :class="{ 'app-shell__content--flush': flush }">
        <slot />
      </div>
    </main>
    <ActivityPanel v-if="dialogs.activityOpen" />

    <CreateRoomDialog
      v-if="dialogs.createRoom"
      :community-id="dialogs.createRoom.communityId"
      @close="dialogs.createRoom = null"
    />
    <CreateCommunityDialog v-if="dialogs.createCommunity" @close="dialogs.createCommunity = false" />
    <AddFriendDialog v-if="dialogs.addFriend" @close="dialogs.addFriend = false" />
  </div>
</template>

<style scoped lang="scss">
.app-shell {
  display: flex;
  height: 100vh;
  width: 100vw;
  background: var(--bg-1);

  &__sidebar {
    width: var(--sidebar-w);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-2);
    border-right: 1px solid var(--line);
  }

  &__main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: radial-gradient(900px 400px at 85% -10%, rgba(123, 97, 255, 0.08), transparent 60%), var(--bg-1);
  }

  &__content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--s-8) var(--s-8) var(--s-10);
    animation: shell-in var(--t-med) var(--ease);

    &--flush {
      padding: 0;
      overflow: hidden;
      display: flex;
      flex-direction: column;
    }
  }
}

@keyframes shell-in {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
}
</style>
