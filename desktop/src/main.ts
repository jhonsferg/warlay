import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
// Self-hosted (no external network fetch, unlike a Google Fonts <link>) —
// both headings and body text use Inter, referenced by name in
// _tokens.scss's $font-heading/$font-body (both point at the same family;
// Inter carries hierarchy through weight, same as San Francisco does on
// real Apple platforms).
import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
// Installed for future UI work but not wired into $font-heading/$font-body
// or applied anywhere yet — see the $font-lato/$font-nunito/$font-roboto/
// $font-ubuntu variables in _tokens.scss.
import "@fontsource/lato/300.css";
import "@fontsource/lato/400.css";
import "@fontsource/lato/700.css";
import "@fontsource/lato/900.css";
import "@fontsource/nunito/400.css";
import "@fontsource/nunito/600.css";
import "@fontsource/nunito/700.css";
import "@fontsource/nunito/800.css";
import "@fontsource/roboto/300.css";
import "@fontsource/roboto/400.css";
import "@fontsource/roboto/500.css";
import "@fontsource/roboto/700.css";
import "@fontsource/ubuntu/300.css";
import "@fontsource/ubuntu/400.css";
import "@fontsource/ubuntu/500.css";
import "@fontsource/ubuntu/700.css";
import "./assets/scss/main.scss";

createApp(App).use(createPinia()).mount("#app");
