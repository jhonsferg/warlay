// Packages the already-built warlay.exe + wnetd.exe into a portable .zip
// (no installer needed) next to the NSIS installer output, so both
// distribution forms live in the same folder -- run after `npm run tauri
// build` (or `tauri build`), which is what actually produces both exes and
// the bundle/nsis directory this writes into.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..", "..");
const releaseDir = path.join(repoRoot, "target", "release");
const bundleNsisDir = path.join(releaseDir, "bundle", "nsis");

const tauriConf = JSON.parse(readFileSync(path.join(here, "..", "src-tauri", "tauri.conf.json"), "utf8"));
const version = tauriConf.version;

const warlayExe = path.join(releaseDir, "warlay.exe");
const wnetdExe = path.join(releaseDir, "wnetd.exe");
const leemeSrc = path.join(here, "portable-zip-leeme.txt");

for (const required of [warlayExe, wnetdExe, leemeSrc, bundleNsisDir]) {
  if (!existsSync(required)) {
    throw new Error(`expected ${required} to exist -- run 'npm run tauri build' first`);
  }
}

const stageDir = mkdtempSync(path.join(tmpdir(), "warlay-portable-"));
copyFileSync(warlayExe, path.join(stageDir, "warlay.exe"));
copyFileSync(wnetdExe, path.join(stageDir, "wnetd.exe"));
copyFileSync(leemeSrc, path.join(stageDir, "LEEME.txt"));

const zipPath = path.join(bundleNsisDir, `Warlay_${version}_x64-portable.zip`);
if (existsSync(zipPath)) rmSync(zipPath);

// Compress-Archive (not a cross-platform zip lib) matches this being a
// Windows-only distributable anyway -- same reasoning as shelling out to
// cargo/rustc in prepare-daemon-sidecar.mjs.
execFileSync(
  "powershell.exe",
  [
    "-NoProfile",
    "-Command",
    `Compress-Archive -Path '${stageDir}\\*' -DestinationPath '${zipPath}' -CompressionLevel Optimal`,
  ],
  { stdio: "inherit" },
);

rmSync(stageDir, { recursive: true, force: true });
console.log(`Portable zip written to ${zipPath}`);
