import { defineStore } from "pinia";
import { ref } from "vue";

export interface ConfirmOptions {
  title: string;
  message: string;
  /** Consequences, one per line, shown under the message. */
  bullets?: string[];
  confirmLabel: string;
  cancelLabel?: string;
  /** "danger" paints the confirm button red and is for irreversible actions. */
  tone?: "danger" | "primary";
}

interface PendingConfirm extends Required<Omit<ConfirmOptions, "bullets">> {
  bullets?: string[];
  resolve: (accepted: boolean) => void;
}

/** App-wide dialogs that can be opened from anywhere. */
export const useDialogs = defineStore("dialogs", () => {
  const createRoom = ref<{ communityId: string | null } | null>(null);
  const createCommunity = ref(false);
  const addFriend = ref(false);
  const confirmRequest = ref<PendingConfirm | null>(null);
  const activityOpen = ref(localStorage.getItem("warlay.activityOpen") !== "false");

  function toggleActivity() {
    activityOpen.value = !activityOpen.value;
    localStorage.setItem("warlay.activityOpen", String(activityOpen.value));
  }

  /** Asks the user to confirm; resolves true only on an explicit accept. */
  function confirm(options: ConfirmOptions): Promise<boolean> {
    // A new question replaces an unanswered one (which counts as "no").
    confirmRequest.value?.resolve(false);
    return new Promise((resolve) => {
      confirmRequest.value = { cancelLabel: "Cancelar", tone: "primary", ...options, resolve };
    });
  }

  function answerConfirm(accepted: boolean) {
    const pending = confirmRequest.value;
    confirmRequest.value = null;
    pending?.resolve(accepted);
  }

  return {
    confirmRequest,
    confirm,
    answerConfirm,
    createRoom,
    createCommunity,
    addFriend,
    activityOpen,
    toggleActivity,
    openCreateRoom: (communityId: string | null = null) => (createRoom.value = { communityId }),
  };
});
