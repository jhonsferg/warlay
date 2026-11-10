import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

// Warlay used to be two programs: the desktop UI and a separate network
// "agent" (a daemon) it talked to over a named pipe. The agent was absorbed:
// everything is warlay.exe now. This guard keeps the old daemon from coming
// back, in code, tests, scripts and docs, so nothing can quietly reintroduce
// it (a pipe, an agent settings panel, `--daemon`, `Agent*` protocol types).

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..", "..");

const SKIP_FILES = new Set(["Cargo.lock", "package-lock.json", "CHANGELOG.md"]); // CHANGELOG records history
const TEXT = /\.(rs|toml|ts|vue|mjs|cjs|js|json|md|yml|yaml|xml|nsh|txt|html|scss|css|sh|gitignore)$|CODEOWNERS$/i;

/** The files the repository actually versions (not local scratch or build output). */
function trackedTextFiles(): string[] {
  const listed = execFileSync("git", ["ls-files", "-z"], { cwd: root, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  return listed
    .split("\u0000")
    .filter((rel) => rel && !rel.startsWith(".claude/") && TEXT.test(rel) && !SKIP_FILES.has(rel.split("/").pop()!))
    .map((rel) => join(root, rel))
    .filter((path) => existsSync(path));
}

/** Things that must not exist anywhere in the repository. */
const FORBIDDEN: { what: string; pattern: RegExp }[] = [
  { what: "the word daemon", pattern: /\bdaemons?\b/i },
  { what: "the network agent", pattern: /\bagentes?\b|\bnetwork agent\b|\bagent (settings|status|state|store|log)/i },
  {
    what: "agent protocol types and commands",
    pattern: /Agent(Request|Response|State|Settings|Store|Status)|[a-z]_agent_(settings|state|status)|agent_store/,
  },
  { what: "the agent's log files", pattern: /warlaid/i },
  { what: "the old pipe", pattern: /named pipe|\\\\\.\\pipe|--daemon|ipc_server/i },
  { what: "the standalone agent crate", pattern: /crates\/(agent|daemon)|\bwnetd\b/i },
];

/** Lines where the word is about something else entirely. */
const ALLOWED_LINE = [
  /user[-_ ]?agent/i, // the HTTP User-Agent header
  /docker daemon|daemon-reload|daemon off/i, // the Docker, systemd and nginx daemons
  /coding agents|AGENTS\.md|\.agents?\//i, // .gitignore: AI coding assistants
];

test("no daemon or network agent is left anywhere in the repository", () => {
  const found: string[] = [];
  const self = fileURLToPath(import.meta.url);
  for (const file of trackedTextFiles()) {
    if (file === self) continue;
    const text = readFileSync(file, "utf8").split(/\r?\n/);
    text.forEach((line, index) => {
      if (ALLOWED_LINE.some((allowed) => allowed.test(line))) return;
      for (const { what, pattern } of FORBIDDEN) {
        if (pattern.test(line))
          found.push(`${relative(root, file)}:${index + 1} (${what}): ${line.trim().slice(0, 100)}`);
      }
    });
  }
  assert.deepEqual(found, [], `traces of the old daemon:\n${found.join("\n")}`);
});

test("the old agent module, crate and files are gone", () => {
  for (const gone of [
    "daemon",
    "crates/agent",
    "desktop/src-tauri/src/agent",
    "desktop/src-tauri/src/commands/agent.rs",
    "desktop/src-tauri/src/commands/matchmaking.rs",
    "crates/engine/src/matchmaking_client.rs",
    "desktop/src/api.ts",
    "desktop/src/types.ts",
  ]) {
    assert.equal(existsSync(join(root, gone)), false, `${gone} must not exist`);
  }
});

test("the settings screens have no network or agent tab of their own", () => {
  const settings = readFileSync(join(root, "desktop/src/pages/SettingsPage.vue"), "utf8");
  assert.doesNotMatch(settings, /Red y agente|Registros'|Actualizaciones'/);
  assert.match(settings, /id: 'advanced', label: t\('Avanzado'\)/);
});

test("every command the UI invokes is registered by the backend, and none of the old hub ones are", () => {
  const lib = readFileSync(join(root, "desktop/src-tauri/src/lib.rs"), "utf8");
  const registered = new Set([...lib.matchAll(/commands::\w+::(\w+),/g)].map((m) => m[1]));
  const native = readFileSync(join(root, "desktop/src/services/native.ts"), "utf8");
  const called = [...native.matchAll(/invoke<[^>]*>\(\s*"(\w+)"/g)].map((m) => m[1]);
  assert.ok(called.length > 20, "found the UI's commands");
  for (const name of called) {
    assert.ok(registered.has(name), `native.ts calls "${name}" but the backend does not register it`);
  }
  for (const gone of [
    "get_agent_settings",
    "save_agent_settings",
    "retry_peer_mesh",
    "connect_to_server",
    "disconnect_from_server",
    "create_room",
    "join_room",
    "start_hosting_tunnel",
    "start_joining_tunnel",
    "stop_hub_tunnel",
    "list_servers",
    "add_server",
  ]) {
    assert.ok(!registered.has(gone), `"${gone}" belonged to the old daemon/hub protocol and must stay removed`);
    assert.ok(!native.includes(`"${gone}"`), `the UI must not call "${gone}"`);
  }
});
