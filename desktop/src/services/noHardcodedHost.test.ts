import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

// The address of the Warlay server is injected at build time (see
// services/serverUrl.ts); it must never be written in the client's source.
// This guard fails if a real host shows up in the app's code, so a forgotten
// literal cannot leak the server (or silently point a fork at ours).

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");

const tracked = execFileSync("git", ["ls-files", "-z"], { cwd: root, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 })
  .split("\u0000")
  .filter(Boolean);

/** The client's own, non-test source. */
const clientSource = tracked.filter(
  (file) =>
    /^desktop\/(src\/.*\.(ts|vue)|src-tauri\/src\/.*\.rs|index\.html|vite\.config\.ts)$/.test(file) &&
    !/\.test\.ts$/.test(file) &&
    !/(^|\/)tests?\.rs$|_tests\.rs$/.test(file),
);

/** Hosts that are fine to name: this project, standards and placeholders. */
const ALLOWED_HOSTS = [
  /^127\.0\.0\.1$/,
  /^localhost$/,
  /^ipc\.localhost$/,
  /^(.+\.)?example\.(com|org|net)$/,
  /^(api\.)?github\.com$/,
  /^evil\.example$/, // a fake host used by the updater's own security tests
  /^www\.w3\.org$/, // SVG / manifest namespaces
  /^schemas\.microsoft\.com$/,
];

const URL_LITERAL = /\b(?:https?|wss?):\/\/([A-Za-z0-9.-]+)/g;
const IPV4_LITERAL = /["'`](\d{1,3}(?:\.\d{1,3}){3})(?::\d+)?["'`]/g;

test("no real server host or address is written in the client's source", () => {
  assert.ok(clientSource.length > 50, "found the client's source files");
  const found: string[] = [];
  for (const file of clientSource) {
    const lines = readFileSync(join(root, file), "utf8").split(/\r?\n/);
    lines.forEach((line, index) => {
      for (const match of line.matchAll(URL_LITERAL)) {
        if (!ALLOWED_HOSTS.some((allowed) => allowed.test(match[1]))) found.push(`${file}:${index + 1}: ${match[0]}`);
      }
      for (const match of line.matchAll(IPV4_LITERAL)) {
        if (
          !/^(127\.|0\.0\.0\.0|192\.168\.|1\.2\.3\.4|10\.|255\.|169\.254\.|224\.|239\.|172\.(1[6-9]|2\d|3[01])\.)/.test(
            match[1],
          )
        ) {
          found.push(`${file}:${index + 1}: ${match[0]}`);
        }
      }
    });
  }
  assert.deepEqual(found, [], `hosts hard-coded in the client:\n${found.join("\n")}`);
});

test("the server address comes only from the build-time variable", () => {
  const http = readFileSync(join(root, "desktop/src/services/http.ts"), "utf8");
  assert.match(http, /VITE_WARLAY_MAIN_URL/);
  assert.doesNotMatch(http, /https?:\/\//, "http.ts must not name any address");
});

test("the build fails without the address and the workflows pass the secret", () => {
  const vite = readFileSync(join(root, "desktop/vite.config.ts"), "utf8");
  assert.match(vite, /serverUrlProblem/);
  assert.match(vite, /throw new Error\(problem\)/);

  const release = readFileSync(join(root, ".github/workflows/release.yml"), "utf8");
  assert.match(release, /VITE_WARLAY_MAIN_URL: \$\{\{ secrets\.WARLAY_MAIN_URL \}\}/);
  assert.match(release, /Require the server URL secret/);

  const ci = readFileSync(join(root, ".github/workflows/ci.yml"), "utf8");
  assert.match(ci, /VITE_WARLAY_MAIN_URL: \$\{\{ secrets\.WARLAY_MAIN_URL \|\|/);
});

test("a local .env is ignored by git and an example is provided", () => {
  const ignore = readFileSync(join(root, ".gitignore"), "utf8");
  assert.match(ignore, /^\.env$/m);
  assert.match(ignore, /^!\.env\.example$/m);
  assert.ok(tracked.includes("desktop/.env.example"), "desktop/.env.example is versioned");
  assert.ok(!tracked.some((file) => /(^|\/)\.env$/.test(file)), "no real .env is versioned");
});

test("Ajustes does not show or edit the server address", () => {
  const advanced = readFileSync(join(root, "desktop/src/ui/organisms/AdvancedSettingsSection.vue"), "utf8");
  assert.doesNotMatch(advanced, /mainServerUrl|Servidor de Warlay|server-url|readonly/i);
  const settings = readFileSync(join(root, "desktop/src/pages/SettingsPage.vue"), "utf8");
  assert.doesNotMatch(settings, /mainServerUrl|setMainServerUrl|Servidor de Warlay/);
  const http = readFileSync(join(root, "desktop/src/services/http.ts"), "utf8");
  assert.doesNotMatch(http, /setMainServerUrl|localStorage\.getItem\(SERVER/);
});
