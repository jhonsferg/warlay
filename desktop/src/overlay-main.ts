import { createApp } from "vue";
import OverlayApp from "./OverlayApp.vue";
// NOT main.scss -- that stylesheet paints an opaque background on <body>
// for the main app window, which broke the overlay's transparency (see
// overlay-reset.scss's own comment).
import "./assets/scss/overlay-reset.scss";

createApp(OverlayApp).mount("#app");
