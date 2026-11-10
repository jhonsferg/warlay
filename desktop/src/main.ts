import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import { router } from "./router";
import { ApiError } from "./services/http";
import { isDesktop, native } from "./services/native";
import { removeLegacyServerOverride } from "./services/serverUrl";
// Self-hosted fonts (no external fetch): Inter for UI text, Rajdhani for
// display headings.
import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/inter/700.css";
import "@fontsource/rajdhani/600.css";
import "@fontsource/rajdhani/700.css";
import "./styles/theme.scss";

// The server address used to be editable in Ajustes; it is fixed by the build now.
removeLegacyServerOverride(localStorage);

const app = createApp(App);
app.use(createPinia()).use(router);

// Errors nobody handled are kept on this machine for the next run to offer as a
// report (see `crash.rs`); nothing is sent. Failing to keep one is not worth another error.
function keepUiError(message: string, stack: string) {
  if (!isDesktop) return;
  void native.recordUiError(message, stack).catch(() => {});
}
app.config.errorHandler = (error, _instance, info) => {
  console.error(error);
  keepUiError(
    String(error instanceof Error ? error.message : error),
    `${error instanceof Error ? (error.stack ?? "") : ""}\n(${info})`,
  );
};
window.addEventListener("error", (event) =>
  keepUiError(event.message, event.error instanceof Error ? (event.error.stack ?? "") : ""),
);
window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason;
  // A refused request or a failed command is an answer, not a crash.
  if (!(reason instanceof Error) || reason instanceof ApiError) return;
  keepUiError(
    `Unhandled rejection: ${reason instanceof Error ? reason.message : String(reason)}`,
    reason instanceof Error ? (reason.stack ?? "") : "",
  );
});

app.mount("#app");

// Dismiss the static splash (index.html) once the first route has resolved,
// which includes restoring the session, so the user never sees a blank frame.
void router.isReady().then(() => {
  const splash = document.getElementById("splash");
  if (!splash) return;
  requestAnimationFrame(() => {
    splash.classList.add("splash--hide");
    splash.addEventListener("transitionend", () => splash.remove(), { once: true });
    window.setTimeout(() => splash.remove(), 600);
  });
});

// Developer tools: release builds ship with them enabled (see
// `app.security.devtools` in tauri.conf.json), but WebView2's own F12
// accelerator is not reliable inside a Tauri window, so the chord is handled
// here and forwarded to the shell. Opening is idempotent (see the
// `open_devtools` command), so both paths firing at once is harmless.
if (isDesktop) {
  window.addEventListener("keydown", (event) => {
    const chord = event.key === "F12" || (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "i");
    if (!chord) return;
    event.preventDefault();
    void native.openDevtools();
  });
}
