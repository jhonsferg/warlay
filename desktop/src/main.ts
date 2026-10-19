import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import { router } from "./router";
import { isDesktop, native } from "./services/native";
// Self-hosted fonts (no external fetch): Inter for UI text, Rajdhani for
// display headings.
import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/inter/700.css";
import "@fontsource/rajdhani/600.css";
import "@fontsource/rajdhani/700.css";
import "./styles/theme.scss";

createApp(App).use(createPinia()).use(router).mount("#app");

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
