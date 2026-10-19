import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";

const MAX_BODY = 120;

/** Whether the user is looking at the app right now. */
export const windowActive = () => document.visibilityState === "visible" && document.hasFocus();

let permission: Promise<boolean> | null = null;

function granted(): Promise<boolean> {
  permission ??= isPermissionGranted()
    .then((ok) => ok || requestPermission().then((answer) => answer === "granted"))
    .catch(() => false);
  return permission;
}

/** An operating-system notification, for when the app is in the background.
 * Silent where there is no Tauri shell (browser, tests) or no permission. */
export async function notifyOs(title: string, body: string) {
  if (!("__TAURI_INTERNALS__" in window) || windowActive()) return;
  if (!(await granted())) return;
  sendNotification({ title, body: body.length > MAX_BODY ? `${body.slice(0, MAX_BODY - 1)}…` : body });
}
