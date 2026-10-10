import { fileURLToPath, URL } from "node:url";
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";

// Every component's own scoped SCSS needs the shared design tokens
// ($gold, $border-iron, etc.) without every single file remembering to
// `@use` them by hand -- forgetting it is how a component ends up
// hardcoding a color that then drifts from the rest of the app. Prepending
// this import via additionalData makes it automatic and uniform instead.
const tokensPath = fileURLToPath(new URL("./src/assets/scss/_tokens.scss", import.meta.url)).replace(/\\/g, "/");
// The design system's own SCSS helpers (its tokens are CSS custom
// properties in src/styles/theme.scss).
const mixinsPath = fileURLToPath(new URL("./src/styles/_mixins.scss", import.meta.url)).replace(/\\/g, "/");

// Tauri expects a fixed, predictable dev server port and no automatic
// browser opening; it drives the webview itself.
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  css: {
    preprocessorOptions: {
      scss: {
        additionalData: `@use "${tokensPath}" as *;\n@use "${mixinsPath}" as *;\n`,
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2022",
    sourcemap: true,
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
        overlay: fileURLToPath(new URL("./overlay.html", import.meta.url)),
      },
    },
  },
});
