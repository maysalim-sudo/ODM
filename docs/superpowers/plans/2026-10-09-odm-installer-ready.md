# ODM: a "Next, Next, Install, enjoy" download for the website

## Context
The user will put an installer on the website, next to the public GitHub repo. A stranger should be able to download it, click through it, and have everything working.

**Today:** `target\release\bundle\nsis\ODM_0.1.0_x64-setup.exe` (4.9 MB, built 10-08 19:29 from `master` 64167e7, current) already installs ODM, the browser helper and the extension files, per user. Three things stop it from being "enjoy right away":
1. **The installer is bare.** It has no licence page, no publisher info, no branded images, and no run-after-install option set on purpose.
2. **Video tools are missing.** FFmpeg and yt-dlp (plus a JS runtime for YouTube) aren't included, so HLS and site videos fail until the user installs them by hand. **Decision:** one-click "Get video tools" inside ODM, which downloads them into ODM's own folder and keeps yt-dlp updated.
3. **The extension can't be added by an installer.** Chrome, Edge and Firefox block that. **Decision:**
   - a guided setup screen in ODM;
   - publish to the Chrome Web Store and Edge Add-ons (the Firefox add-on is already in review), so users click "Add to Chrome" without Developer mode.

**Constraints from this session:**
- Anything that must touch the user's real registry or install for real is run by the user, not by me (Claude's MSIX container virtualizes HKCU).
- The user's ODM locks `target\release` while it runs.
- Gates on every task: Rust fmt, clippy `-D warnings`, workspace tests; app `pnpm check` (0/0), `pnpm test`, `pnpm build`; extension `pnpm check`, `pnpm test`, `pnpm build`.
- One commit per task. Push `master` after each task (single-branch preference).

## Task 1: Installer polish
**Files:** `app/src-tauri/tauri.conf.json`, `app/src-tauri/installer-hooks.nsh`, new `app/src-tauri/installer/` (BMP images).

- **`bundle`:**
  - `publisher: "Salim May"`, `copyright: "© 2026 Salim May. MIT License."`, `homepage: "https://github.com/salimmay/ODM"`;
  - `shortDescription` and `longDescription` (from the README's first lines);
  - `licenseFile: "../../LICENSE"` (shows the licence page);
  - `windows.webviewInstallMode: {"type": "downloadBootstrapper"}`, so Windows 10 without WebView2 still works.
- **`bundle.windows.nsis`:**
  - keep `installMode: "currentUser"` and the hooks;
  - add `installerIcon: "icons/icon.ico"`;
  - `headerImage` (150×57) and `sidebarImage` (164×314) BMPs in the black and orange theme. Generate them from `app/app-icon.svg` plus text with a small script (Pillow, or ffmpeg drawtext); commit the BMPs.
- **Run after install:** Tauri's NSIS finish page has a "Run ODM" checkbox, on by default. Confirm it's there; if a hook is needed, add `NSIS_HOOK_POSTINSTALL` only for the extension-folder note.
- **Uninstall:** keeps the `--unregister` hook. Also remove the managed tools folder (Task 2) via `NSIS_HOOK_POSTUNINSTALL`: `RMDir /r "$LOCALAPPDATA\ODM\tools"`. Downloads and the database are left alone.

## Task 2: One-click video tools
**New:** `crates/odm-appcore/src/tools.rs`.
**Touches:**
- `crates/odm-engine/src/hls/merge.rs` (`find_ffmpeg`, `resolve_ffmpeg`);
- `crates/odm-engine/src/site/mod.rs` (`find_ytdlp`, `find_node`, `build_args`);
- `crates/odm-engine/src/config.rs` (`tools_dir: Option<PathBuf>`);
- `crates/odm-appcore/src/settings.rs` (`to_engine_config` sets `tools_dir`);
- `app/src-tauri/src/commands.rs` and `lib.rs`;
- UI: `SettingsDialog.svelte`, `DetailPanel.svelte`, a new `ToolsCard.svelte`.

**Where the tools go:** `%LOCALAPPDATA%\ODM\tools\`, an `AppPaths::tools_dir` (honours `ODM_DATA_DIR` for tests).

**Sources** (HTTPS, each checked against its published SHA-256):
| Tool | URL | Checksum source | Kept |
|---|---|---|---|
| yt-dlp | `https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe` | `…/latest/download/SHA2-256SUMS` | `yt-dlp.exe` |
| FFmpeg | `https://github.com/yt-dlp/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip` | `…/latest/checksums.sha256` | `ffmpeg.exe`, `ffprobe.exe` from `bin/` |
| Deno (YouTube's JS runtime) | `https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip` | `….zip.sha256sum` | `deno.exe` |

**`tools.rs`:**
- `status(dir) -> ToolsStatus { ffmpeg, ytdlp, deno: Option<String /*version*/> }` (runs `--version` with `CREATE_NO_WINDOW`, 5 s timeout).
- `install(dir, client, on_progress) -> Result<(), String>`. It downloads to `*.part`, verifies the SHA-256, extracts with the `zip` crate (new dependency), then renames. A failure leaves the existing tools untouched.
- Exact messages:
  - "Could not download the video tools. Check your connection and try again.";
  - "The downloaded file didn't match its checksum, so it wasn't installed.".
- `update_ytdlp_if_stale(dir)`: if the managed `yt-dlp.exe` is older than 7 days, run `yt-dlp.exe -U` in the background at app start, logging the outcome.

**Lookup order** (engine), for FFmpeg and yt-dlp alike:
1. the configured path;
2. `tools_dir`;
3. `PATH`;
4. the winget locations.

`build_args` uses `--js-runtimes deno:<tools_dir>\deno.exe` when present, else node as today.

**Tauri:** commands `tools_status()` and `tools_install()`, the latter emitting `UiEvent::ToolsProgress { stage, done, total }` and running on a blocking task.

**UI:**
- **Settings, "Video tools" card:** status lines ("FFmpeg 7.1 ✓", "yt-dlp 2026.08.19 ✓", "Not installed") and a "Get video tools" button with a progress bar showing roughly the 150 MB download.
- **Detail panel:** when a download failed with the yt-dlp-not-found or FFmpeg-not-found message, show a "Get video tools" button, then "Retry".

**Tests (first):**
- **`install`** against `odm-testserver` serving a fake `yt-dlp.exe`, a zip and checksum files:
  - the happy path installs all three;
  - a checksum mismatch → the exact message, and nothing installed;
  - a mid-download failure → existing tools are kept.
- **Lookup order:** `tools_dir` beats `PATH`, and the configured path beats `tools_dir`.
- **`build_args`:** picks deno when it's present.
- **`model.ts`:** a `toolsSummary(status)` helper.

## Task 3: Allow the store extension IDs
The Chrome Web Store and Edge Add-ons each assign their **own** extension ID on first upload, different from the dev ID (`aijkcebmfjiicjdfchfbdehejghbpadb`). The helper must accept all of them.

- `extension/chromium-ids.json`: `{"dev": "<current>", "chrome_web_store": null, "edge_addons": null}`. The existing `extension/chromium-id.txt` stays the dev ID.
- `odm_core::ids::chromium_ext_ids() -> Vec<&'static str>` (`include_str!` plus a tiny parse; non-null entries).
- `odm-host` `register.rs`: `chromium_manifest` takes a slice and writes every `chrome-extension://<id>/` into `allowed_origins`.
- **Tests:** a manifest with 1 ID and with 3 IDs; the existing `manifests_match_spec` is updated for the list form.
- **Extension store build:** `pnpm build:store` writes `dist/store-chromium` without the `"key"` field (the stores reject it) and zips it to `target/store/odm-extension-chromium-store.zip`.

## Task 4: Store listing package
New `store/` folder, committed:
- **`PRIVACY.md`:** no data collected; cookies and URLs go only to ODM on the same PC. It's linked from the README and used as the store privacy-policy URL (the GitHub blob URL).
- **`chrome-web-store.md` and `edge-addons.md`:** name, short and long descriptions, category (Productivity), single purpose, a justification for each permission (adapted from `extension/AMO-NOTES.md`), "no remote code", support URL.
- **Images:**
  - 1280×800 screenshots retaken in the black and orange theme with the demo setup (`scratchpad/demoserver.py`, debug build with `--config identifier com.odm.demo`, the `scrub.js` path and host scrubbing): the main list, the chunk map, the New download dialog, the video button on a page, and the popup with Video detection;
  - a 440×280 small promo tile;
  - the 128 px icon.

**User steps (only you can do these):**
1. Create a Chrome Web Store developer account (one-time $5) and an Edge Partner Center account (free).
2. Upload `odm-extension-chromium-store.zip` as a **draft** to each, and send me the two IDs shown.
3. I fill the IDs into `chromium-ids.json`, rebuild, and you submit for review with the texts and images from `store/`.

## Task 5: First-run guided setup
**New:** `app/src/components/WelcomeDialog.svelte`.
**Touches:** `settings.rs` (`onboarding_done: bool`, default false), `commands.rs` (`open_browser_extensions_page(browser)`), `App.svelte`, `BrowserDialog.svelte` (a "Setup guide" link).

**Shown** on first start (when `onboarding_done` is false), and from the Browser connection panel. Three steps on one screen:
1. **Your browser:** Chrome, Edge, Brave, Vivaldi, Opera, Firefox, Floorp.
2. **Add the extension:**
   - When a store listing is known (`chromium-ids.json` IDs, plus the AMO URL once approved): an **"Add to <browser>"** button opening the store page.
   - Otherwise: **"Open extension folder"**, then **"Open extensions page"** (it launches the browser exe with `chrome://extensions`, `edge://extensions`, `vivaldi://extensions`, `about:debugging#/runtime/this-firefox`; exe paths from the registry `App Paths` or the usual install folders), plus the 3 short Load-unpacked steps.
   - A live line: "Waiting for your browser…" → "Connected ✓". It polls `integration_status().last_contact_ms` every 2 s.
3. **Video tools:** a "Get video tools" button (Task 2), with "Skip for now".

"Done" saves `onboarding_done = true`.

**Tests (first):** `model.ts` `storeLinkFor(browser, ids)` and `extensionsPageFor(browser)`; the Rust command maps browser → exe + URL in a pure fn, which is unit-tested.

## Task 6: Rebuild, check, publish
1. **The user quits ODM.** I run all the gates, then `node scripts/release.mjs` → a new installer.
2. **Installer page check** (I can't drive the user's real installer): build, then the user runs it and confirms:
   - licence → folder → install → finish with "Run ODM";
   - the welcome screen appears;
   - "Get video tools" works;
   - after loading the extension, "Connected ✓".
3. `target\release-assets\`:
   - the installer;
   - the Chromium (dev) zip and the Firefox zip;
   - the Chromium store zip;
   - the AMO source zip;
   - `SHA256SUMS.txt`.
4. **On the user's "yes, publish":**
   - make the repo public;
   - tag `v0.1.0`;
   - `gh release create` with the assets and notes. The notes cover install steps, the SmartScreen "More info → Run anyway" warning (unsigned), video tools, and the extension stores "coming soon" until approved.
5. **Website:** link `https://github.com/salimmay/ODM/releases/latest/download/ODM_0.1.0_x64-setup.exe`.
   - **Website note:** the filename includes the version, so for a link that never changes, publish one more copy of the installer in the release as `ODM-setup.exe` and link `…/latest/download/ODM-setup.exe`.
   - Update the project-page artifact's buttons, the extension menu (store links when live) and the screenshots (orange).

## Verification
- **Automated:** all the gates green, including the new tools-install tests (local server), the multi-ID manifest tests and the onboarding helper tests.
- **Build:** the installer exists. `7z l` isn't available, so I'll check the bundle through `target\release\` resources (`extension\`, `odm-host.exe`) and the NSIS config.
- **Live (user):** the step 6.2 walkthrough on a fresh install. Optionally, also test on a Windows 10 VM or Sandbox (Windows Sandbox gives a clean machine with no WebView2/FFmpeg: the best real "stranger" test).
- **Publish:** the release lists every asset, and the `latest/download` links resolve (`curl -I` → 302).

## Not in scope
- Code signing. SmartScreen keeps warning until a certificate is bought, or a free OSS signer such as SignPath is set up later.
- Auto-update of ODM itself.
- DASH.
