import { fileURLToPath, URL } from "node:url";
import vue from "@vitejs/plugin-vue";
import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import { serverUrlProblem, SERVER_URL_VAR } from "./src/services/serverUrl";

const { version } = JSON.parse(readFileSync(fileURLToPath(new URL("./package.json", import.meta.url)), "utf8")) as {
  version: string;
};

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
  plugins: [
    vue(),
    // The server address is injected at build time and is not in the source:
    // a production build without a valid one must fail here, not ship an app
    // that cannot reach any server.
    {
      name: "require-server-url",
      configResolved(config) {
        if (config.command !== "build") return;
        const problem = serverUrlProblem(config.env[SERVER_URL_VAR] as string | undefined);
        if (problem) throw new Error(problem);
      },
    },
    // Stamps the app version into the splash screen in index.html.
    {
      name: "splash-version",
      transformIndexHtml: (html: string) => html.replace("%APP_VERSION%", version),
    },
  ],
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
      },
    },
  },
});
