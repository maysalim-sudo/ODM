// Builds the ODM installer. Run from the repo root: node scripts/release.mjs
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const shell = process.platform === "win32";

function fail(step, detail) {
  console.error(`\nrelease failed at: ${step}${detail ? `\n${detail}` : ""}`);
  process.exit(1);
}

function run(step, cmd, args, cwd = root) {
  console.log(`\n==> ${step}`);
  const r = spawnSync(cmd, args, { cwd, stdio: "inherit", shell });
  if (r.error) fail(step, String(r.error));
  if (r.status !== 0) fail(step, `exit code ${r.status}`);
}

run("cargo build --release -p odm-host", "cargo", ["build", "--release", "-p", "odm-host"]);

console.log("\n==> copy odm-host into the app's binaries");
const rustc = spawnSync("rustc", ["-vV"], { encoding: "utf8", shell });
if (rustc.status !== 0) fail("rustc -vV", rustc.stderr);
const triple = /^host:\s*(\S+)/m.exec(rustc.stdout)?.[1];
if (!triple) fail("rustc -vV", "no host line in the output");
const binDir = join(root, "app", "src-tauri", "binaries");
mkdirSync(binDir, { recursive: true });
try {
  copyFileSync(join(root, "target", "release", "odm-host.exe"), join(binDir, `odm-host-${triple}.exe`));
} catch (e) {
  fail("copy odm-host", String(e));
}

const ext = join(root, "extension");
run("extension: pnpm install", "pnpm", ["install", "--frozen-lockfile"], ext);
run("extension: pnpm build", "pnpm", ["build"], ext);

const app = join(root, "app");
run("app: pnpm install", "pnpm", ["install", "--frozen-lockfile"], app);
run("app: pnpm tauri build", "pnpm", ["tauri", "build"], app);

const nsisDir = join(root, "target", "release", "bundle", "nsis");
let installers = [];
try {
  installers = readdirSync(nsisDir).filter((f) => f.endsWith(".exe"));
} catch {
  // reported below
}
if (installers.length === 0) fail("find the installer", `nothing in ${nsisDir}`);

console.log("\nDone.");
for (const f of installers) console.log(`Installer: ${join(nsisDir, f)}`);
console.log(`Extension (Chromium): ${join(ext, "dist", "chromium")}`);
console.log(`Extension (Firefox):  ${join(ext, "dist", "firefox")}`);
