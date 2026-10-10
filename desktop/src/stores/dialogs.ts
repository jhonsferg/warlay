import { defineStore } from "pinia";
import { ref } from "vue";

/** App-wide dialogs that can be opened from anywhere. */
export const useDialogs = defineStore("dialogs", () => {
  const createRoom = ref<{ communityId: string | null } | null>(null);
  const createCommunity = ref(false);
  const addFriend = ref(false);
  const activityOpen = ref(localStorage.getItem("warlay.activityOpen") !== "false");

  function toggleActivity() {
    activityOpen.value = !activityOpen.value;
    localStorage.setItem("warlay.activityOpen", String(activityOpen.value));
  }

  return {
    createRoom,
    createCommunity,
    addFriend,
    activityOpen,
    toggleActivity,
    openCreateRoom: (communityId: string | null = null) => (createRoom.value = { communityId }),
  };
});
