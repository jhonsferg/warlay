import { watch } from "vue";
import { native } from "../services/native";
import { notifyOs } from "../services/osNotify";
import { realtime } from "../services/realtime";
import { useBlocks } from "./blocks";
import { useChat } from "./chat";
import { acceptInviteAndOpen } from "./inviteFlow";
import { useE2e } from "./e2e";
import { useCommunities } from "./communities";
import { useNotifications } from "./notifications";
import { useRooms } from "./rooms";
import { useSession } from "./session";
import { useSocial } from "./social";
import { useToasts } from "./toasts";
import { t } from "../services/i18n";

let started = false;
let unsubscribe: (() => void) | null = null;

/** Loads everything for a signed-in user and wires realtime events to the
 * stores. Called once the session is known; undone on sign out. */
async function start() {
  if (started) return;
  started = true;
  const session = useSession();
  const social = useSocial();
  const communities = useCommunities();
  const rooms = useRooms();
  const chat = useChat();
  const notifications = useNotifications();
  const toasts = useToasts();

  unsubscribe = realtime.on((event) => {
    switch (event.type) {
      case "hello":
        // A fresh socket (first connect, or after moving to another edge):
        // whatever was pushed while we weren't connected is lost, so
        // reload the state the events would have updated.
        void Promise.allSettled([
          social.load(),
          notifications.load(),
          rooms.loadList(),
          communities.loadMine(),
          rooms.resyncSession(),
          chat.catchUp(),
          chat.loadUnread(),
          useBlocks().load(),
        ]);
        break;
      case "presence":
        social.applyPresence(event.userId, event.presence);
        break;
      case "friendRequest":
        social.applyRequest(event.request, session.me?.id ?? "");
        if (!("removed" in event.request) && event.request.to.id === session.me?.id) {
          void notifyOs(t("Nueva solicitud de amistad"), event.request.from.displayName);
          const request = event.request;
          // Like Facebook: answer right from the toast. The X only closes it --
          // the request stays under the bell until it is answered.
          toasts.action(
            t("Nueva solicitud de amistad"),
            t("{name} quiere ser tu amigo", { name: request.from.displayName }),
            [
              { label: t("Aceptar"), run: () => social.accept(request.id) },
              { label: t("Rechazar"), variant: "ghost", run: () => social.decline(request.id) },
            ],
          );
        }
        break;
      case "friendAdded":
        social.applyFriendAdded(event.friend);
        break;
      case "friendRemoved":
        social.applyFriendRemoved(event.userId);
        break;
      case "notification":
        notifications.applyNotification(event.notification);
        if (event.notification.kind === "friend_accepted") toasts.success(t("Solicitud de amistad aceptada"));
        break;
      case "invite":
        notifications.applyInvite(event.invite);
        {
          const invite = event.invite;
          toasts.action(
            invite.kind === "room" ? t("Te invitaron a una sala") : t("Te invitaron a una comunidad"),
            `${invite.inviter.displayName} · ${invite.targetName}`,
            [
              { label: t("Aceptar"), run: () => acceptInviteAndOpen(invite) },
              { label: t("Rechazar"), variant: "ghost", run: () => notifications.declineInvite(invite.id) },
            ],
          );
        }
        break;
      case "message":
        chat.receive(event.message);
        break;
      case "messageDeleted":
        chat.applyDeleted(event.channel, event.messageId);
        break;
      case "chatReceipt":
        chat.applyReceipt(event.channel, event.userId, event.receipt);
        break;
      case "roomUpdated":
        if (rooms.applyUpdated(event.room) === "kicked") toasts.error(t("Te expulsaron de la sala"));
        break;
      case "roomHelperChanged":
        rooms.applyHelperChanged(event.roomId, event.helper);
        break;
      case "roomClosed":
        if (rooms.applyClosed(event.roomId)) toasts.info(t("La sala se cerró"));
        break;
      case "communityUpdated":
        communities.refreshSoon(event.communityId);
        break;
      case "sessionRevoked":
        void session.logout();
        break;
    }
  });
  // Before the socket: its first messages may be encrypted.
  void useE2e().init();
  realtime.connect();

  await Promise.allSettled([
    social.load(),
    communities.loadMine(),
    rooms.loadList(),
    notifications.load(),
    useBlocks().load(),
    rooms.restoreCurrent(),
  ]);
}

function stop() {
  if (!started) return;
  started = false;
  unsubscribe?.();
  unsubscribe = null;
  realtime.disconnect();
  useSocial().reset();
  useCommunities().reset();
  useRooms().reset();
  useChat().reset();
  useE2e().reset();
  useNotifications().reset();
  useBlocks().reset();
}

/** The host's game started/ended the match: reflect it on the room. */
async function wireNative() {
  const rooms = useRooms();
  await native.onGameStarted(() => void rooms.setStatus("in_game"));
  await native.onGameEnded(() => void rooms.setStatus("open"));
}

export function installAppLifecycle() {
  const session = useSession();
  void wireNative();
  watch(
    () => session.status,
    (status) => {
      if (status === "signedIn") void start();
      else if (status === "signedOut") stop();
    },
    { immediate: true },
  );
}
