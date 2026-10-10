import { defineStore } from "pinia";
import { ref } from "vue";
import type { ChatMessageEntry } from "../types";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { Window } from "@tauri-apps/api/window";
import { playNotification } from "../utils/audio";

export interface Friend {
  clientId: string;
  displayName: string;
}

export const useChatStore = defineStore("chat", () => {
  const friends = ref<Map<string, Friend>>(new Map());
  const privateMessages = ref<Map<string, ChatMessageEntry[]>>(new Map());

  function addFriend(clientId: string, displayName: string) {
    if (!friends.value.has(clientId)) {
      friends.value.set(clientId, { clientId, displayName });
      if (!privateMessages.value.has(clientId)) {
        privateMessages.value.set(clientId, []);
      }
    }
  }

  function removeFriend(clientId: string) {
    friends.value.delete(clientId);
    privateMessages.value.delete(clientId);
  }

  async function notifyUser(title: string, body: string) {
    let granted = await isPermissionGranted();
    if (!granted) {
      const permission = await requestPermission();
      granted = permission === "granted";
    }
    if (granted) {
      // Check if window is focused
      const appWindow = Window.getCurrent();
      const isFocused = await appWindow.isFocused();
      if (!isFocused) {
        sendNotification({ title, body });
      }
    }
  }

  function handleIncomingPrivateMessage(entry: ChatMessageEntry, toClientId: string, myClientId: string) {
    // If it's sent TO me, add it to the sender's conversation
    // If it's sent BY me, add it to the recipient's conversation
    const isMine = entry.fromClientId === myClientId;
    const conversationId = isMine ? toClientId : entry.fromClientId;

    // Automatically add to friends if they message you
    if (!isMine && !friends.value.has(conversationId)) {
      addFriend(conversationId, entry.fromDisplayName);
    }

    if (!privateMessages.value.has(conversationId)) {
      privateMessages.value.set(conversationId, []);
    }
    privateMessages.value.get(conversationId)?.push(entry);

    if (!isMine) {
      void notifyUser(`Mensaje de ${entry.fromDisplayName}`, entry.text);
      playNotification();
    }
  }

  return {
    friends,
    privateMessages,
    addFriend,
    removeFriend,
    handleIncomingPrivateMessage,
    notifyUser,
  };
});
