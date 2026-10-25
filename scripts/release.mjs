// Builds the ODM installers. Run from the repo root: node scripts/release.mjs
//
// On Windows: the NSIS installer. On Linux: .deb, .rpm and .AppImage, copied to
// target/release-assets-linux/ as ODM-amd64.deb, ODM-x86_64.rpm and ODM-x86_64.AppImage.
// --updater-pubkey <file> compiles that public key in without signing (Linux builds use it so
// they can verify later updates; the private key stays where the signed Windows build runs).
//
// --publish-ready also runs every gate first, signs the build for the updater and writes
// target/release-assets/ (latest.json, the versioned installer and ODM-setup.exe). It needs
// TAURI_SIGNING_PRIVATE_KEY_PATH (the key from `tauri signer generate -w <path>`, with its
// public half at <path>.pub) and TAURI_SIGNING_PRIVATE_KEY_PASSWORD.
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildLatestJson, staleReleaseFiles } from "./latest-json.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const shell = process.platform === "win32";
const isWindows = process.platform === "win32";
const isLinux = process.platform === "linux";
const publishReady = process.argv.includes("--publish-ready");
const pubkeyArg = process.argv.indexOf("--updater-pubkey");
const pubkeyOnlyPath = pubkeyArg > 0 ? process.argv[pubkeyArg + 1] : undefined;

function fail(step, detail) {
  console.error(`\nrelease failed at: ${step}${detail ? `\n${detail}` : ""}`);
  process.exit(1);
}

function run(step, cmd, args, cwd = root, env = process.env) {
  console.log(`\n==> ${step}`);
  const r = spawnSync(cmd, args, { cwd, stdio: "inherit", shell, env });
  if (r.error) fail(step, String(r.error));
  if (r.status !== 0) fail(step, `exit code ${r.status}`);
}

if (!isWindows && !isLinux) fail("platform", `ODM builds on Windows and Linux, not ${process.platform}.`);
if (isLinux && publishReady) {
  fail(
    "--publish-ready",
    "Signed releases are built on Windows. On Linux, build with --updater-pubkey <file.pub> so the\n" +
      "packages can verify later updates, then add them to the release made on Windows.",
  );
}
if (pubkeyArg > 0 && !pubkeyOnlyPath) fail("--updater-pubkey", "Give the path of the .pub file after it.");

/** The signing key and its public half, or a refusal that says what is missing. */
function signingKey() {
  const keyPath = process.env.TAURI_SIGNING_PRIVATE_KEY_PATH;
  const password = process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD;
  const missing = [];
  if (!keyPath) missing.push("TAURI_SIGNING_PRIVATE_KEY_PATH");
  if (password === undefined) missing.push("TAURI_SIGNING_PRIVATE_KEY_PASSWORD");
  if (missing.length > 0) {
    fail(
      "signing key",
      `--publish-ready needs ${missing.join(" and ")}. See "Releasing" in README.md.\n` +
        "Without the key, existing installs can't verify the update, so nothing is built.",
    );
  }
  if (!existsSync(keyPath)) fail("signing key", `No private key at ${keyPath}.`);
  const pubPath = `${keyPath}.pub`;
  if (!existsSync(pubPath)) {
    fail("signing key", `No public key at ${pubPath}. It is written next to the key by "tauri signer generate -w".`);
  }
  const pubkey = readFileSync(pubPath, "utf8").trim();
  if (!pubkey) fail("signing key", `${pubPath} is empty.`);
  // The repo carries the public key for builds that only verify updates (Linux, --updater-pubkey).
  // It must be the half of the key that signs, or those builds would refuse every update.
  const repoPub = join(root, "app", "src-tauri", "odm-updater.pub");
  if (existsSync(repoPub) && readFileSync(repoPub, "utf8").trim() !== pubkey) {
    fail("signing key", `${repoPub} is not the public half of ${keyPath}. Copy ${pubPath} over it and commit.`);
  }
  return { keyPath, password, pubkey };
}

const key = publishReady ? signingKey() : null;
/** A public key compiled in without signing (--updater-pubkey). */
function pubkeyOnly() {
  if (!pubkeyOnlyPath) return null;
  if (!existsSync(pubkeyOnlyPath)) fail("--updater-pubkey", `No public key at ${pubkeyOnlyPath}.`);
  const pubkey = readFileSync(pubkeyOnlyPath, "utf8").trim();
  if (!pubkey) fail("--updater-pubkey", `${pubkeyOnlyPath} is empty.`);
  return pubkey;
}
const pubkeyAlone = key ? null : pubkeyOnly();
const version = JSON.parse(readFileSync(join(root, "app", "src-tauri", "tauri.conf.json"), "utf8")).version;
const changelog = publishReady ? readFileSync(join(root, "CHANGELOG.md"), "utf8") : "";
if (publishReady) {
  // Checked before the long build, with a throwaway signature.
  try {
    buildLatestJson({ version, changelog, signature: "-", date: new Date() });
  } catch (e) {
    fail("CHANGELOG.md", e.message);
  }
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
  const exe = isWindows ? ".exe" : "";
  copyFileSync(join(root, "target", "release", `odm-host${exe}`), join(binDir, `odm-host-${triple}${exe}`));
} catch (e) {
  fail("copy odm-host", String(e));
}

const ext = join(root, "extension");
run("extension: pnpm install", "pnpm", ["install", "--frozen-lockfile"], ext);
run("extension: pnpm build", "pnpm", ["build"], ext);

const app = join(root, "app");
run("app: pnpm install", "pnpm", ["install", "--frozen-lockfile"], app);

if (publishReady) {
  // Every gate passes before anything is signed. Clippy needs the extension built above.
  run("gate: cargo fmt", "cargo", ["fmt", "--all", "--", "--check"]);
  run("gate: cargo clippy", "cargo", ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]);
  run("gate: cargo test", "cargo", ["test", "--workspace", "--", "--test-threads=4"]);
  run("gate: extension check", "pnpm", ["check"], ext);
  run("gate: extension test", "pnpm", ["test"], ext);
  run("gate: app check", "pnpm", ["check"], app);
  run("gate: app test", "pnpm", ["test"], app);
}

if (isLinux) {
  const linuxAssets = join(root, "target", "release-assets-linux");
  mkdirSync(linuxAssets, { recursive: true });
  const args = ["tauri", "build", "--bundles", "deb,rpm,appimage"];
  if (pubkeyAlone) {
    const overlay = join(linuxAssets, "updater-config.json");
    writeFileSync(overlay, JSON.stringify({ plugins: { updater: { pubkey: pubkeyAlone } } }, null, 2));
    args.push("--config", overlay);
  }
  run(`app: pnpm tauri build (deb, rpm, AppImage${pubkeyAlone ? ", with the update key" : ""})`, "pnpm", args, app);
  const bundle = join(root, "target", "release", "bundle");
  const pick = (dir, suffix) => {
    let found = [];
    try {
      found = readdirSync(join(bundle, dir)).filter((f) => f.endsWith(suffix) && f.includes(version));
    } catch {
      // reported below
    }
    if (found.length !== 1) fail(`find the ${suffix} package`, `expected one ${version} file in ${join(bundle, dir)}, found ${found.length}`);
    return join(bundle, dir, found[0]);
  };
  const outputs = [
    [pick("deb", ".deb"), "ODM-amd64.deb"],
    [pick("rpm", ".rpm"), "ODM-x86_64.rpm"],
    [pick("appimage", ".AppImage"), "ODM-x86_64.AppImage"],
  ];
  for (const [from, name] of outputs) copyFileSync(from, join(linuxAssets, name));
  console.log("\nDone.");
  for (const [from, name] of outputs) console.log(`${name}  <-  ${from}`);
  console.log(`Linux packages: ${linuxAssets}`);
  console.log(pubkeyAlone ? "Update key: compiled in." : "Update key: none (update checks are off in this build).");
  process.exit(0);
}

const assets = join(root, "target", "release-assets");
mkdirSync(assets, { recursive: true });
const nsisDir = join(root, "target", "release", "bundle", "nsis");
const setupName = `ODM_${version}_x64-setup.exe`;
console.log("\n==> remove updater files left by an earlier build");
for (const f of staleReleaseFiles({ assets, nsisDir, setupName })) {
  try {
    rmSync(f, { force: true });
  } catch (e) {
    fail("remove stale files", `${f}: ${e}`);
  }
}
if (key) {
  // The public key is compiled in only here; the committed config has none.
  const overlay = join(assets, "updater-config.json");
  writeFileSync(
    overlay,
    JSON.stringify({ bundle: { createUpdaterArtifacts: true }, plugins: { updater: { pubkey: key.pubkey } } }, null, 2),
  );
  const env = {
    ...process.env,
    TAURI_SIGNING_PRIVATE_KEY: readFileSync(key.keyPath, "utf8").trim(),
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: key.password,
  };
  // Relative to app/: the shell splits an absolute path at the spaces in "Data From Desktop".
  run("app: pnpm tauri build (signed)", "pnpm", ["tauri", "build", "--config", relative(app, overlay)], app, env);
} else {
  run("app: pnpm tauri build", "pnpm", ["tauri", "build"], app);
}

let installers = [];
try {
  installers = readdirSync(nsisDir).filter((f) => f.endsWith(".exe"));
} catch {
  // reported below
}
if (installers.length === 0) fail("find the installer", `nothing in ${nsisDir}`);

const setup = join(nsisDir, setupName);
if (!existsSync(setup)) fail("find the installer", `no ${setupName} in ${nsisDir}`);
// The website links to ODM-setup.exe, which always is the latest version.
copyFileSync(setup, join(assets, "ODM-setup.exe"));

if (publishReady) {
  const sigPath = `${setup}.sig`;
  if (!existsSync(sigPath)) fail("updater signature", `no ${sigPath}; the build was not signed`);
  let manifest;
  try {
    manifest = buildLatestJson({ version, changelog, signature: readFileSync(sigPath, "utf8"), date: new Date() });
  } catch (e) {
    fail("latest.json", e.message);
  }
  copyFileSync(setup, join(assets, setupName));
  copyFileSync(sigPath, join(assets, `${setupName}.sig`));
  writeFileSync(join(assets, "latest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
}

console.log("\nDone.");
for (const f of installers) console.log(`Installer: ${join(nsisDir, f)}`);
console.log(`Website copy: ${join(assets, "ODM-setup.exe")}`);
if (publishReady) {
  console.log(`Release assets (upload all to the v${version} release): ${assets}`);
  console.log(`  latest.json, ${setupName}, ${setupName}.sig, ODM-setup.exe`);
}
console.log(`Extension (Chromium): ${join(ext, "dist", "chromium")}`);
console.log(`Extension (Firefox):  ${join(ext, "dist", "firefox")}`);
