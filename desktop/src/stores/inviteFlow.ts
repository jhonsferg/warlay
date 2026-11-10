import { router } from "../router";
import { errorMessage } from "../services/http";
import type { InviteView } from "../services/types";
import { useCommunities } from "./communities";
import { useNotifications } from "./notifications";
import { useRooms } from "./rooms";
import { useToasts } from "./toasts";
import { t } from "../services/i18n";

/**
 * Accepting an invitation, wherever the button is (the bell or the toast that
 * announced it): join, then take the user there. Returns whether it worked.
 */
export async function acceptInviteAndOpen(invite: InviteView): Promise<boolean> {
  const notifications = useNotifications();
  const rooms = useRooms();
  const communities = useCommunities();
  const toasts = useToasts();
  try {
    const result = await notifications.acceptInvite(invite.id);
    if (result.kind === "room") {
      await rooms.adopt(result.session);
      void router.push(`/room/${result.session.room.id}`);
    } else {
      await communities.loadMine();
      void router.push(`/c/${result.communityId}`);
    }
    return true;
  } catch (error) {
    toasts.error(t("No se pudo aceptar la invitación"), errorMessage(error));
    return false;
  }
}
