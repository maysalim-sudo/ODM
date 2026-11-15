# Developing ODM

How to build, test and release ODM. For using it, see the [README](../README.md).

## Requirements

- Windows 10 or 11 with the WebView2 runtime (included in Windows 11)
- Rust (stable, MSVC toolchain) and the Visual Studio C++ Build Tools
- Node 22.12 or newer and pnpm
- TypeScript is pinned to version 6 because the current `svelte-check` does not accept TypeScript 7.

## Building the installer

    node scripts/release.mjs

This builds the helper, the extension and the app, then prints the paths. Run the installer from `target\release\bundle\nsis\`. It installs for the current user, so it needs no admin rights, and it is unsigned, so Windows SmartScreen may ask you to confirm.

- ODM registers the browser helper (`odm-host.exe`, installed next to `ODM.exe`) the first time it starts.
- Uninstalling removes that registration.
- Load the extension from `extension\dist\chromium` or `extension\dist\firefox` as described under "Browser integration".

This plain build has no update key, so it never checks for updates (Settings → Updates says "Updates are not available in this build."). Release builds are made with `--publish-ready`, below.

## Releasing

Releases are signed for the updater: an installed ODM only installs an update whose signature matches the public key compiled into it.

**Once: make the signing key.** In `app\`, run:

    pnpm tauri signer generate -w %USERPROFILE%\.tauri\odm-updater.key

Choose a password. This writes the private key to `%USERPROFILE%\.tauri\odm-updater.key` and the public key next to it, as `odm-updater.key.pub`. Neither goes into the repo: `release.mjs` reads the public key from the `.pub` file and compiles it in at build time.

> **Back up the key file and its password** (for example in a password manager, plus an offline copy). If you lose either, you can't sign updates any more, and every installed copy of ODM stops updating: users would have to download and install the next version by hand.

**Each release:**

1. Raise `version` in `app\src-tauri\tauri.conf.json`, `Cargo.toml` (then `cargo update --workspace --offline` for `Cargo.lock`), `app\package.json`, `extension\package.json` and both `extension\manifests\*.json`.
2. In `CHANGELOG.md`, turn "Unreleased" into a section for that version, like `## [0.2.0] - 2026-10-20`. Its text becomes the "What's new" of the update.
3. Build, in PowerShell:

       powershell -NoProfile -ExecutionPolicy Bypass -File scripts\sign-release.ps1

   It asks for the key password (hidden), checks it against the key in a few seconds (3 tries), and only then runs `node scripts\release.mjs --publish-ready` with the key and password set for that build alone. Setting `TAURI_SIGNING_PRIVATE_KEY_PATH` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` yourself and running `release.mjs --publish-ready` directly also works. The release script refuses to start without both variables or without the `.pub` file, runs every check and test first, and stops at the first failure. Then it builds the signed installer and writes `target\release-assets\`: `latest.json`, `ODM_<version>_x64-setup.exe`, its `.sig`, and `ODM-setup.exe` (the same installer under the stable name the website links to).
4. Zip the extension: `extension/dist/chromium` and `extension/dist/firefox` with `extension/scripts/zip.mjs` (`odm-extension-chromium.zip`, `odm-extension-firefox.zip`), and the source with `git archive HEAD:extension` (`odm-extension-source.zip`). Write `SHA256SUMS.txt` for every file except the `.sig` files and `latest.json`.
5. Create the GitHub release `v<version>` as a draft, upload this version's files from `target\release-assets\`, the Linux files below, the extension zips and `SHA256SUMS.txt`, then publish it. `target\release-assets\` also keeps older versions, so upload only this version's files. Installed copies read the `latest.json` of the latest release (the updater `endpoints` in `app/src-tauri/tauri.conf.json`), so the update reaches them once the release is published (not as a draft or a pre-release).

**Linux packages** (`.deb`, `.rpm`, `.AppImage`), built on Ubuntu 22.04, Kubuntu or Debian, or in a Docker container of one (build them before the Windows build):

1. Once per machine or image: `bash scripts/linux-setup.sh` (asks your password once, for `apt`; installs the build libraries, Rust, Node and pnpm for your user). In Docker, set `APPIMAGE_EXTRACT_AND_RUN=1` so the AppImage tools run without FUSE.
2. In the repo: `node scripts/release.mjs --updater-pubkey app/src-tauri/odm-updater.pub`. It writes `target/release-assets-linux/`: `ODM-amd64.deb`, `ODM-x86_64.rpm` and `ODM-x86_64.AppImage` for the website, the same three as `ODM_<version>_amd64.AppImage`, `ODM_<version>_amd64.deb` and `ODM_<version>_x86_64.rpm` for the updater, with the update key compiled in so they can verify later updates. Nothing is signed on Linux; the private key stays on the Windows machine.
3. Copy that folder to `target\release-assets-linux\` on the Windows machine **before** the signed Windows build. That build then signs the three versioned packages too, copies them and their `.sig` files into `target\release-assets\`, and lists them in `latest.json` (`linux-x86_64-appimage`, `-deb`, `-rpm`), so Linux copies update in the app: an AppImage replaces itself, a `.deb` or `.rpm` install asks for the system password. Without the folder, `latest.json` covers Windows only.
4. Add all of the Linux files to the same GitHub release as the Windows files.

`app/src-tauri/odm-updater.pub` is the public half of the update key. When the key changes, copy the new `.pub` over it and commit; the signed Windows build refuses to run while they differ.

To test an update locally, set `ODM_UPDATE_ENDPOINT` to another `latest.json` before starting ODM. It accepts an https address, or plain http on this PC (`localhost`, `127.0.0.1`); the signature is still required.

## Run the desktop app

    cd app
    pnpm install
    pnpm tauri dev

Build a standalone executable (written to `target/debug/odm-app.exe`):

    cd app
    pnpm tauri build --debug --no-bundle

The app bundles the built extension (`extension/dist`) and Tauri fails the build if it is missing, so build the extension first (see "Browser integration"), as `node scripts/release.mjs` does. This also applies to `cargo check` and `cargo clippy` of the whole workspace. In ODM, the status bar's **Browser** status opens a panel that shows what is registered, tests the connection, repairs the registration and opens the bundled extension folders.

## Browser integration

1. Build the browser helper next to the app: `cargo build -p odm-host` (or `--release` for a release build of the app). ODM registers it for Chrome, Edge, Brave, Vivaldi, Opera, Chromium, Firefox and Floorp the first time it starts, and again whenever it is moved.
2. Build the extension:

       cd extension
       pnpm install
       pnpm build

3. Chrome, Edge and other Chromium browsers: open `chrome://extensions`, turn on Developer mode, choose **Load unpacked** and pick `extension/dist/chromium`. Its ID is fixed (see `extension/chromium-id.txt`), which is what lets the helper talk to it.
4. Firefox and Floorp: open `about:debugging`, choose **This Firefox → Load Temporary Add-on** and pick `extension/dist/firefox/manifest.json`. A temporary add-on is removed when the browser restarts. To keep it, sign it as an unlisted add-on with your own addons.mozilla.org keys (`npx web-ext sign --channel=unlisted --source-dir dist/firefox`), or use a build that allows unsigned add-ons. In Firefox, open the ODM popup once and choose **Allow** so it can read download cookies.

Using it:
- Click a download link as usual. ODM takes it with the page's cookies and Referer, and the browser's own download disappears. If ODM doesn't answer within 5 seconds, the browser keeps the download.
- Hold **Alt** while clicking to let the browser download it instead.
- Small files (under the minimum capture size), sites on the skip list, and everything while capture is off stay in the browser. All three are in ODM's Settings.
- Right-click a link, video or audio element and choose **Download with ODM**.
- The toolbar popup shows whether ODM is running, the active and queued counts, the total speed, and Pause all / Resume all / Open ODM.
- The extension popup can turn video detection off for the current site or everywhere; downloads are still taken over.
- If ODM is closed, a captured download starts it in the tray.

`odm-host --unregister` removes the registration.

## Command line

    cargo run -p odm-cli --release -- get <URL> [--dir DIR] [--name NAME] [-c CONNECTIONS] [--limit BYTES_PER_SECOND]

## Tests

    cargo test --workspace -- --test-threads=4
    cd app && pnpm test && pnpm check
    cd extension && pnpm test && pnpm check

## Layout

| Path | What it is |
|---|---|
| `crates/odm-core` | Shared types: states, errors, file names, categories |
| `crates/odm-store` | SQLite storage |
| `crates/odm-engine` | The download engine and queue |
| `crates/odm-torrent` | BitTorrent and magnet links (the only crate that uses librqbit) |
| `crates/odm-appcore` | App logic that doesn't depend on Tauri: settings, logging, UI data, event bridge |
| `crates/odm-cli` | Command-line tool |
| `crates/odm-host` | Browser helper (native messaging host) and its registration |
| `crates/odm-testserver` | Test-only HTTP server that can misbehave on purpose |
| `app/src-tauri` | The Tauri shell: commands, tray, window |
| `app/src` | The Svelte user interface |
| `extension` | The browser extension (Chromium and Firefox builds) |
| `docs/superpowers` | Design spec and implementation plans |
