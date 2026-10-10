// Builds the daemon in release mode and copies it into src-tauri/binaries
// under the platform-suffixed name Tauri's `externalBin` sidecar mechanism
// expects (`<name>-<target-triple>.exe`), so `tauri build` actually ships
// wnetd.exe inside the installer instead of relying on it merely existing
// in the same folder at dev time.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..", "..");
const srcTauri = path.resolve(here, "..", "src-tauri");
const binariesDir = path.join(srcTauri, "binaries");

const targetTriple = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .split("\n")
  .find((line) => line.startsWith("host:"))
  .split(":")[1]
  .trim();

console.log(`Building daemon (release) for ${targetTriple}...`);
execFileSync("cargo", ["build", "--release", "-p", "daemon"], {
  cwd: repoRoot,
  stdio: "inherit",
});

const builtExe = path.join(repoRoot, "target", "release", "wnetd.exe");
if (!existsSync(builtExe)) {
  throw new Error(`expected ${builtExe} to exist after cargo build --release -p daemon`);
}

mkdirSync(binariesDir, { recursive: true });
const sidecarPath = path.join(binariesDir, `wnetd-${targetTriple}.exe`);
copyFileSync(builtExe, sidecarPath);
console.log(`Copied daemon sidecar to ${sidecarPath}`);
