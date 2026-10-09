# ODM: auto-updates, a richer Settings page, bandwidth control

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:**
- **Updates:** people who installed ODM learn about new versions and update in one click.
- **Settings:** a proper Settings page with the options a daily download manager needs: bandwidth control, a scheduler, file-exists policy, proxy, start with Windows, video quality and more.
- **Default folder:** `%USERPROFILE%\Downloads\ODM`.

**Design source:** the `emil-design-skills:apple-design` principles. The cited ones are:
- §1 response;
- §3 interruptibility;
- §4 springs;
- §7 spatial consistency;
- §14 reduced motion;
- §15 typography;
- §16 simplicity ("show the common path first, advanced one level deeper"), grouping and mapping, inline validation.

Theme: the black and orange tokens in `app/src/app.css`.

## Global Constraints
- **Gates:** Rust fmt, clippy `-D warnings`, workspace tests; app `pnpm check` 0/0, `pnpm test`, `pnpm build`; extension `pnpm check`, `pnpm test`, `pnpm build`.
- **Tests:** test-first with real RED runs. One commit per task. Push `master` after each task passes.
- **Settings compatibility:** stays tolerant. Unknown or invalid fields fall back to defaults with a log warning (existing `Settings::from_json`). Every new field has a default, so old `settings.json` files load unchanged.
- **Existing users keep their saved download folder.** Only the default for new installs changes.
- **Real effects are user-run, not run by Claude:** anything touching the real registry (start with Windows), real installs or real updates. Claude's MSIX container virtualizes HKCU.
- **UI copy:** sentence case, plain words, no exclamation marks.
- **Motion:**
  - default springs are critically damped (no overshoot), about 300–400 ms;
  - only `transform` and `opacity`;
  - every motion has a `prefers-reduced-motion` cross-fade equivalent;
  - nothing blocks input during a transition.

## Review Focus
1. **An old `settings.json`** (none of the new keys) loads with every new default and doesn't touch `download_dir`. *Test: Task 2 `old_settings_file_keeps_folder_and_gets_new_defaults`.*
2. **A schedule window crossing midnight** (22:00–06:00) and day-of-week boundaries. *Test: Task 3 `window_across_midnight`.*
3. **An update file whose signature doesn't match** is refused, nothing is installed, and the user sees a plain error. *Test: Task 1 verification step with a tampered `latest.json`.*
4. **A proxy URL that is invalid** (missing scheme, bad port) is rejected inline and never saved. *Test: Task 4 `proxy_url_validation`.*
5. **"Ask what to do" when a file exists, with ODM hidden in the tray.** The prompt brings the window forward and the download waits instead of failing. *Test: Task 3 `exists_ask_waits_for_answer`.*

---

### Task 1: Auto-update

**Files:**
- `app/src-tauri/Cargo.toml` (`tauri-plugin-updater`, `tauri-plugin-process`);
- `tauri.conf.json` (`bundle.createUpdaterArtifacts: true`, `plugins.updater.pubkey`, `plugins.updater.endpoints`, `plugins.updater.windows.installMode: "passive"`);
- `capabilities/default.json`;
- `app/src-tauri/src/updates.rs` (new);
- `crates/odm-appcore/src/settings.rs`;
- `scripts/release.mjs`;
- UI: `UpdateBanner.svelte` (new), Settings → Updates.

**Signing key:** done once, by the user.
- Run `pnpm tauri signer generate -w %USERPROFILE%\.tauri\odm-updater.key` and choose a password.
- The **public** key goes into `tauri.conf.json` (committed). The **private** key and its password stay out of the repo; `release.mjs` reads them from `TAURI_SIGNING_PRIVATE_KEY_PATH` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- `release.mjs` refuses to build an updater release without them.
- README "Releasing" gets a back-up warning: losing the key means existing users can't update.

**Endpoint:** `https://github.com/salimmay/ODM/releases/latest/download/latest.json`.

**Release script:**
- `release.mjs` gains `--publish-ready`. It runs all gates first and stops on any failure.
- It builds with signing, then writes `target/release-assets/latest.json`:
  - `version` from `tauri.conf.json`;
  - `notes` from `CHANGELOG.md`'s top section (new file, Keep-a-Changelog style);
  - `pub_date` in RFC 3339;
  - `platforms."windows-x86_64": { signature: <contents of the .sig>, url: "https://github.com/salimmay/ODM/releases/download/v<version>/ODM_<version>_x64-setup.exe" }`.
- It keeps copying `ODM-setup.exe` (the stable website link).

**Settings** (Updates section):
- `update_check: bool` (default true): "Check for updates automatically";
- `update_install: "ask" | "auto"` (default "ask"): "When an update is ready: Ask me / Install when I quit ODM".

**Behaviour (`updates.rs`):**
- Check 10 s after start, then every 6 h, only if `update_check` is on.
- A found update emits `UiEvent::UpdateAvailable { version, notes, date }` and sends one Windows notification per version: "ODM <v> is available".
- **Download** shows progress in the banner. **Install** finishes the active downloads' current writes first (the existing shutdown path persists and pauses them), then runs the installer (passive) and restarts. Paused downloads resume after the restart, because the queue persists.
- **"auto":** download in the background, install when the user quits ODM.
- **Settings → Updates:**
  - "Check now";
  - the current version;
  - "Last checked 2 h ago";
  - "What's new": the release notes rendered as plain text, with no HTML from the network.
- Test hook: env `ODM_UPDATE_ENDPOINT` overrides the endpoint, so tests can use a local server. The signature is still required.

**Update banner** (`UpdateBanner.svelte`):
- Placement: a slim bar at the top of the content area, above the table; never a modal.
- Text: "ODM 0.2.0 is available." Buttons: "What's new", "Update", and "Later" (hides it until the next version).
- Motion: it slides down 100% of its height with a critically damped spring (~350 ms) and leaves the same way (§7). Reduced motion: a 150 ms fade.
- Progress: "Downloading update… 42%", with a thin orange bar on the banner's bottom edge.

**Tests:**
- Rust: the `latest.json` builder (pure fn in `scripts/release.mjs`, or a tiny Rust/JS module tested with vitest);
- version comparison and "notify once per version" (`updates.rs` pure fns);
- settings defaults.

**Verification:**
1. Build **0.1.0** and install it in a temp folder (user-run, or Windows Sandbox).
2. Build **0.1.1** signed and serve `latest.json` plus the installer from a local server.
3. Set `ODM_UPDATE_ENDPOINT` and confirm: the notification, the banner, Update, the passive install, the restart into 0.1.1, and that downloads survive.
4. Tamper with the signature in `latest.json` and confirm: "The update couldn't be verified, so it wasn't installed." Nothing changes.

**Commit:** `feat: automatic updates with signed releases`

### Task 2: Settings page, Apple-style

Replace the modal `SettingsDialog.svelte` with a full-window `SettingsPage.svelte`. It has a left section list, and one section shows at a time.

**Layout:**
- **Opening:** Settings slides in from the right edge, where its button lives, and leaves to the right (§7). The motion is a spring, critically damped, about 350 ms, transform only. Reduced motion: a cross-fade.
- **Sections, in this order (§16, most common first):** General, Downloads, Speed, Browser, Video, Network, Updates, Advanced.
- **Rows:** label on the left, control on the right, a one-line muted description underneath (§16 grouping and mapping). Rows are grouped into rounded cards (`--surface`, 8 px radius, 1 px `--border` dividers between rows).
- **Controls:** switches for on/off (a new `Switch.svelte`, sharing the extension popup switch's look and contrast rules), segmented controls for 2–4 choices, number fields with units, and folder pickers.
- **Applying:** changes apply instantly, with no Save button (§16, Agency).
  - Invalid values show an inline message under the field and are not applied.
  - Each section has "Restore defaults" at the bottom, with an Undo toast for 5 s; no confirm dialog (Agency: forgiveness over confirmation).
- **Search:** a small search field at the top of the section list filters rows by label and description.
- **Navigation:** Escape or "Done" closes; Ctrl+, opens.
- **Type:** section titles use 20 px weight 700 with tracking −0.01em, row labels 14 px weight 600, descriptions 12.5 px muted (§15).

**Sections and settings** (new fields marked **new**; every new field has a default):

| Section | Setting | Control | Default |
|---|---|---|---|
| General | Start ODM with Windows (**new** `start_with_windows`) | switch | off |
| | Start minimized to the tray | switch | off |
| | Keep running in the tray when closed | switch | on |
| | Notifications: when a download finishes (**new** `notify_complete`), when one fails (**new** `notify_failed`) | switches | on, on |
| | Prevent sleep while downloading (**new** `prevent_sleep`) | switch | on |
| Downloads | Download folder | folder picker | `%USERPROFILE%\Downloads\ODM` (**changed default**) |
| | Sort into folders by type, plus the per-type folder names (existing `category_folders`; **new** `sort_by_type`) | switch + names | on |
| | When a file already exists (**new** `on_exists`) | segmented: Rename / Overwrite / Ask | Rename |
| | When a download finishes (**new** `on_complete`) | segmented: Nothing / Open folder / Open file | Nothing |
| | Start new downloads (**new** `start_mode`) | segmented: Right away / Paused | Right away |
| | Downloads at once | number 1–10 | 3 |
| | Remove finished downloads from the list after (**new** `history_days`) | Never / 7 / 30 / 90 days | Never |
| Speed | Speed limit (existing `speed_limit_kbps`, shown with units) | switch + number + KB/s or MB/s | off |
| | Limit only at certain times (**new** `limit_schedule`): days + from/to | day chips + 2 time fields | off |
| | Connections per download | number 1–32 | 8 |
| | Run the queue at certain times (**new** `queue_schedule`): start at, stop at, days | switch + times | off |
| | When the queue is done (**new** `after_queue`) | segmented: Nothing / Sleep / Shut down / Quit ODM | Nothing |
| Browser | Take over downloads from the browser | switch | on |
| | Ask where to save each one | switch | on |
| | Only take files larger than | number + MB | 1 MB |
| | Leave these file types to the browser (**new** `skip_types`, e.g. `pdf, jpg`) | tag field | empty |
| | Sites to leave to the browser | tag field (existing `skip_hosts`) | empty |
| | Browser connection: status line + "Open connection panel" | link | — |
| Video | Video tools: status + Get/Update (existing card) | card | — |
| | Preferred quality (**new** `video_quality`) | segmented: Best / 1080p / 720p / 480p / Audio only | Best |
| | Format (**new** `video_container`) | segmented: MP4 / MKV | MP4 |
| | Download subtitles when available (**new** `video_subtitles`) | switch | off |
| | Keep yt-dlp up to date automatically (**new** `ytdlp_auto_update`) | switch | on |
| Network | Proxy (**new** `proxy_mode`, `proxy_url`) | segmented: System / None / Manual + URL (`http://`, `socks5://`) | System |
| | Connect timeout, read timeout | numbers (existing) | 30 s, 60 s |
| | Retries per part | number 1–20 (existing) | 5 |
| Updates | Task 1's settings, plus "Check now" and the version | | |
| Advanced | Custom User-Agent (**new** `user_agent`, empty = default) | text | empty |
| | FFmpeg path, yt-dlp path (existing) | file pickers | empty |
| | Open the logs folder, open the data folder | buttons | — |
| | Reset all settings | button, with Undo toast | — |

**Engine and appcore support** (each with its tests):
- **`EngineConfig.download_dir` default:** `home\Downloads\ODM`. `Settings::default` follows.
  - *Test:* `default_download_dir_is_downloads_odm`.
  - *Test:* `old_settings_file_keeps_folder_and_gets_new_defaults` (Review Focus 1).
- **`sort_by_type = false`:** `save_dir_for` returns the download folder itself.

**Commit:** `feat(ui): Settings page with sections, instant apply, new options` (the engine pieces for the new options land in Tasks 3–4; until each one does, its row is hidden behind a `ready` flag so the UI never shows a control that does nothing).

### Task 3: Download behaviour options (engine)
- **`on_exists`:**
  - **Rename:** today's `unique_name`.
  - **Overwrite:** replaces the existing file under `FINISH_LOCK`.
  - **Ask:** the download pauses in a new `Waiting` reason. It emits `UiEvent::ExistsPrompt { id, name }`; the UI shows "<name> already exists" with the choices Replace / Keep both / Cancel.
  - *Tests:* `exists_rename`, `exists_overwrite`, `exists_ask_waits_for_answer` (Review Focus 5).
- **`on_complete`:** after Completed, `open_folder` / `open_file` through the opener. A risky file type never auto-opens; it opens its folder instead.
- **`start_mode = Paused`:** UI and IPC adds start paused (except the New download dialog's explicit "Start now").
- **`history_days`:** a daily pass removes Completed records older than N days. **Records only, never files.** *Test with a fake clock.*
- **Schedules:**
  - a pure `schedule::active(now, window) -> bool` with days and from/to, including windows that cross midnight. *Tests:* `window_across_midnight`, `day_filter` (Review Focus 2).
  - The manager ticks every 30 s:
    - **speed:** the limit applies only inside `limit_schedule`;
    - **queue:** outside `queue_schedule` nothing new starts, and running downloads pause at the stop time (a "Paused by schedule" reason, so Resume all doesn't fight the schedule).
- **`after_queue`:** when the scheduled queue empties, act:
  - **Sleep:** `SetSuspendState`.
  - **Shut down:** `shutdown /s /t 60`, with a cancellable notification "ODM will shut down the PC in 60 s." and a Cancel button.
  - **Quit ODM:** quits.
  - Only real side effects: tests use an injected `PowerActions` trait.
- **`prevent_sleep`:** `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)` while any download is active, cleared when none are.
- **Per-download speed limit** (the engine already has a `local_limit` path):
  - Detail panel: a "Speed limit" row (Unlimited / custom).
  - New download dialog: an optional "Limit speed" field.
- **`skip_types`:** a capture filter in `capture::skip_reason`, with the message "this file type is on the skip list".
- **Commit:** `feat(engine): file-exists policy, schedules, after-queue actions, per-download limit`

### Task 4: Network and video options
- **Proxy:** `build_client` uses `reqwest::Proxy`.
  - **System:** reqwest's default, which follows the Windows/WinINet proxy through env and the system configuration. If system proxy discovery isn't available in reqwest on Windows, read `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings` `ProxyServer` / `ProxyEnable`.
  - **None:** `no_proxy`.
  - **Manual:** the URL.
  - **yt-dlp** gets `--proxy <url>` when the proxy is Manual.
  - *Test:* `proxy_url_validation` (Review Focus 4).
  - *Test:* a manual proxy is used. Requires a tiny CONNECT proxy in `odm-testserver`.
- **User-Agent override** for HTTP requests and for yt-dlp (`--user-agent`). The browser-captured UA still wins for captured downloads.
- **Video options → `build_args`:**
  - quality → `-f`: Best `bv*+ba/b`; 1080p `bv*[height<=1080]+ba/b[height<=1080]`; 720p and 480p the same pattern; Audio only `ba/b` + `-x --audio-format mp3`;
  - container → `--merge-output-format mp4|mkv`;
  - subtitles → `--write-subs --sub-langs all --embed-subs`.
  - *Tests:* exact arg vectors for each.
- **`ytdlp_auto_update` off:** skip the weekly `-U`.
- **Start with Windows:** `tauri-plugin-autostart`, via HKCU Run with `--background`.
  - *Test:* settings round-trip.
  - Real effect: user-verified.
- **Commit:** `feat: proxy, user agent, video quality and format, start with Windows`

### Task 5: Speed control in the main window
- **A speed chip in the toolbar,** right side next to Settings:
  - it shows the total speed ("4.2 MB/s"), plus a small orange dot when a limit is active;
  - clicking opens a popover anchored to the chip, scaling from its top-right corner (`transform-origin` at the trigger, §7), with a critically damped spring and a reduced-motion fade;
  - the popover offers Unlimited, 512 KB/s, 1 MB/s, 2 MB/s, 5 MB/s, Custom… and a "Pause all / Resume all" row;
  - the selection applies instantly.
- **Tray menu:** a "Speed limit" submenu with the same presets.
- **Keyboard:** the popover is a menu with arrow keys, Enter and Escape.
- *Tests:* `speedPresets` and `formatLimit` in `model.ts`; the tray preset mapping as a pure Rust fn.
- **Commit:** `feat(ui): speed chip with quick limits; tray speed menu`

### Task 6: Rebuild, verify, release notes
1. The user quits ODM. Run all gates, then `node scripts/release.mjs --publish-ready` (needs the signing key from Task 1).
2. Live check, done by the user:
   - the Settings page in each section;
   - set the speed limit from the chip, and the speed obeys within a few seconds;
   - turn "Start with Windows" on;
   - schedule the queue a few minutes ahead and watch it start;
   - Windows Sandbox: install 0.1.0, then update to 0.1.1 from a test release.
3. Update `CHANGELOG.md`, the README (new options, updates, the default folder), and the project-page artifact (new screenshots).

## Not in scope
- A light theme.
- Languages other than English.
- A beta update channel.
- Code signing. SmartScreen still warns, but updates are verified by the updater signature, and installing an update through ODM doesn't trigger SmartScreen again.
