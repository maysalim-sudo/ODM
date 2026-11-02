# Extension Fixes and Per-Download Formats Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make failed browser takeovers visible, let people move or hide the on-page controls, and offer MP3 or video for each detected download.

**Architecture:** Extension logic goes into pure functions under `extension/src/lib/`, with `content.ts`, `background.ts` and `popup.ts` staying thin. A nullable JSON `video_override` column carries the choice through the engine, so it survives pause and restart. Plain files and HLS gain one FFmpeg step before `finalize`.

**Tech Stack:** TypeScript and Vitest (extension), Rust, rusqlite, tokio, yt-dlp, FFmpeg.

**Spec:** `docs/superpowers/specs/2026-11-02-odm-suite-design.md`, section "Queue and format additions". Mockup: `design/odm-suite/odm-suite.figcraft`, page Extension.

## Global Constraints

- The user's installed ODM is running downloads. Never start, stop, install or update ODM on Windows. Never run `pnpm tauri dev` or an ODM binary on Windows: debug builds share its pipe and data folder.
- Until the user says the downloads are done, Windows `cargo test` runs only with `-p odm-store -p odm-engine -p odm-core`. Run `odm-appcore`, `odm-host` and whole-workspace tests in WSL (`Ubuntu-22.04`), from a clone in the Linux filesystem.
- Work on branch `feat/extension-formats`, not `master`. Leave the untracked `2026-10-27` plan alone. Ask before any push.
- Repo stores LF. Keep copy plain. A download with no override behaves exactly as today.
- Checks: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings` (WSL), `cd extension && pnpm test`.

## Review Focus

1. **Bar saved off-screen:** a position saved on a large monitor, opened on a small window, must still show the whole bar. Task 3 test.
2. **Old extension, new app:** a payload without `format` must add the download as before. Task 6 test.
3. **Corrupt override:** an unreadable `video_override` value must fall back to Settings, not fail the download. Task 4 test.
4. **Name clash after conversion:** `song.mp3` already existing must go through the normal exists rule, not overwrite. Task 5 test.
5. **Conversion fails:** the downloaded file stays and the download reports failed. Task 5 test.

---

### Task 1: Show when ODM does not answer

**Files:**
- Modify: `extension/src/lib/capture.ts`, `extension/src/background.ts:247`
- Test: `extension/src/lib/capture.test.ts`

**Interfaces:**
- Produces: `captureProblem(r: OdmResponse | null): string | null`

- [ ] **Step 1: Write the failing test** `captureProblem`: `null` returns `"ODM did not answer, so your browser kept this download. Check that ODM is installed and open it once."`; `{ ok: true }` returns `null`; an error with code `skipped` returns `null`; any other error returns `errorText(r)`.
- [ ] **Step 2: Run** `cd extension && pnpm test capture`. Expected: FAIL, `captureProblem` not defined.
- [ ] **Step 3: Implement it** and replace the branch in `handle` so any non-null result goes to `flagError`. Keep the `console.warn`.
- [ ] **Step 4: Run** the same test. Expected: PASS.
- [ ] **Step 5: Commit** `fix(extension): show when ODM does not answer a takeover`.

### Task 2: Trace the Vivaldi takeover in WSL

Investigation, following `superpowers:systematic-debugging`. This task changes no code unless the cause is proven.

**Files:**
- Create: `docs/superpowers/plans/2026-11-02-vivaldi-linux-findings.md`

- [ ] **Step 1:** In `Ubuntu-22.04`, clone the repo to `~/odm`, read `scripts/linux-setup.sh`, then run it. Expected: `pnpm tauri build --debug` works.
- [ ] **Step 2:** Install Vivaldi's `.deb` and a second Chromium browser, and start ODM and the extension there. Expected: both windows open through WSLg.
- [ ] **Step 3:** Start the same test download in both browsers. Record: the manifest path and `path` value, ODM's browser status, `onCreated` and `onDeterminingFilename` firing, and the `sendDetailed` result from the service worker console.
- [ ] **Step 4:** Write the findings with the failing layer named. If a cause is proven, add a failing test and a fix as a new task and get the user's OK. If it does not reproduce, say so and ask the user for the three checks from their Linux install.
- [ ] **Step 5: Commit** `docs: Vivaldi takeover findings on Linux`.

### Task 3: Move and hide the on-page controls

**Files:**
- Create: `extension/src/lib/barPosition.ts`, `extension/src/lib/barPosition.test.ts`
- Modify: `extension/src/content.ts:64-131` (bar), `:184-195,239-285` (hover button), `extension/src/lib/wiring.test.ts` if it pins this markup

**Interfaces:**
- Produces: `type BarPosition = { right: number; bottom: number }`, `BAR_KEY = "odmBarPosition"`, `clampBar(p: BarPosition, bar: { width: number; height: number }, view: { width: number; height: number }): BarPosition`, `nudgeBar(p: BarPosition, key: string): BarPosition | null`

- [ ] **Step 1: Write the failing tests.** `clampBar({ right: 5000, bottom: 5000 }, { width: 200, height: 40 }, { width: 800, height: 600 })` equals `{ right: 592, bottom: 552 }` (8 px margin); negative values clamp to 8; the default `{ right: 16, bottom: 16 }` is unchanged. `nudgeBar` moves 16 px for each arrow key and returns `null` for other keys.
- [ ] **Step 2: Run** `pnpm test barPosition`. Expected: FAIL.
- [ ] **Step 3: Implement** both functions.
- [ ] **Step 4: Run** the same test. Expected: PASS.
- [ ] **Step 5: Change `content.ts`.** Add a focusable grip button labelled "Move" to `.bar`. Use pointer capture on it; set `.box` `right` and `bottom` from `clampBar`; save to `chrome.storage.local` on pointer up and on arrow keys; read on `build`; re-clamp on `resize`. A storage failure falls back to the default. Add a × button to the hover button, titled "Hide for this page", which sets a page-lifetime flag that `show` checks.
- [ ] **Step 6: Run** `pnpm test && pnpm build`. Expected: PASS. Load `dist` in Chrome on the user's machine, or in WSL, and confirm the bar moves, stays after reload, and the × works. ODM is not needed for this.
- [ ] **Step 7: Commit** `feat(extension): move the video bar, hide the video button`.

### Task 4: Store and read a per-download override

**Files:**
- Modify: `crates/odm-store/src/migrations.rs` (`V5`), `crates/odm-store/src/models.rs`, `crates/odm-store/src/repo.rs` (`COLS`, `to_record`, insert)
- Modify: `crates/odm-engine/src/config.rs`, `crates/odm-engine/src/site/mod.rs:118-182`; `crates/odm-engine/src/manager.rs:35-90,770-800`
- Test: the existing `odm-store` tests, `site/mod.rs` tests

**Interfaces:**
- Produces: `#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)] struct VideoOverride { quality: Option<Quality>, container: Option<Container>, subtitles: Option<bool> }` in `odm-engine`; `DownloadRecord.video_override: Option<String>`; `fn apply_override(base: &VideoOptions, stored: Option<&str>) -> VideoOptions`; a `video_override: Option<VideoOverride>` field on `AddRequest`, `StreamRequest` and `SiteRequest`.

- [ ] **Step 1: Write the failing tests.** A store round trip keeps `video_override`, and a V4 database migrates to `None`. `apply_override` with `{ quality: Audio }` gives MP3 args and keeps the configured container; with `None` or `"not json"` it equals the base. `output_name` ends `.mp3` when the quality is Audio.
- [ ] **Step 2: Run** `cargo test -p odm-store -p odm-engine`. Expected: FAIL to compile.
- [ ] **Step 3: Implement.** Append `const V5: &str = "ALTER TABLE downloads ADD COLUMN video_override TEXT;";` and add it to `MIGRATIONS`. `build_args` calls `format_args(&apply_override(&cfg.video, rec.video_override.as_deref()))`. The three add paths serialize the override into the record. `output_name` gains a `audio: bool` parameter.
- [ ] **Step 4: Run** the same tests. Expected: PASS.
- [ ] **Step 5: Commit** `feat(engine): per-download video override`.

### Task 5: Convert plain files and HLS to MP3

**Files:**
- Create: `crates/odm-engine/src/convert.rs`
- Modify: `crates/odm-engine/src/hls/merge.rs` (share the process runner), `crates/odm-engine/src/download.rs:764`, `crates/odm-engine/src/hls/run.rs:461`, `crates/odm-engine/src/lib.rs`
- Test: `crates/odm-engine/tests/behaviour.rs`, plus unit tests in `convert.rs`

**Interfaces:**
- Consumes: `apply_override`, `resolve_ffmpeg(cfg)`
- Produces: `fn wants_mp3(rec: &DownloadRecord) -> bool`, `fn mp3_name(name: &str) -> String`, `async fn to_mp3(ffmpeg: &Path, input: &Path, output: &Path) -> Result<(), OdmError>`

- [ ] **Step 1: Write the failing tests.** `mp3_name("clip.mp4") == "clip.mp3"`, `mp3_name("clip") == "clip.mp3"`. `wants_mp3` is true only for `quality: audio`. With a stub FFmpeg that exits 1, the source stays and the error is returned. With no FFmpeg, the error text starts `FFmpeg not found`. A finished MP3 whose name exists follows the configured exists policy.
- [ ] **Step 2: Run** `cargo test -p odm-engine convert`. Expected: FAIL.
- [ ] **Step 3: Implement.** Args: `-hide_banner -y -i <input> -vn -c:a libmp3lame -q:a 2 <output>`, using `CREATE_NO_WINDOW` on Windows as `merge.rs` does. Before each `finalize`, when `wants_mp3`, emit state `merging`, convert beside the input, delete the input only on success, then finalize with `mp3_name`.
- [ ] **Step 4: Run** the same tests. Expected: PASS.
- [ ] **Step 5: Commit** `feat(engine): convert plain files and HLS to MP3`.

### Task 6: Carry the format from the extension to the engine

**Files:**
- Modify: `crates/odm-appcore/src/ipc.rs` (`AddPage`, stream and download payloads, `add_page`, `add_stream`, `add_download`), `extension/src/lib/protocol.ts`, `extension/src/lib/streams.ts`, `extension/src/lib/sites.ts`, `extension/src/background.ts:77-135`
- Test: `crates/odm-appcore` IPC tests (WSL), `extension/src/lib/streams.test.ts`, `extension/src/lib/sites.test.ts`

**Interfaces:**
- Produces: `type VideoFormat = "video" | "mp3"`; an optional `format` on `AddPagePayload`, stream and download payloads; `#[serde(default)] format: Option<VideoFormat>`; `fn override_for(format: Option<VideoFormat>) -> Option<VideoOverride>`; `format` on the `odm-add-page`, `odm-add-stream` and `odm-add-video` messages.

- [ ] **Step 1: Write the failing tests.** A payload without `format` gives `video_override: None`. `"mp3"` gives `quality: Audio`. `"video"` on a site page gives `container: Mp4`; on a file it gives `None`. `toPagePayload` includes `format` only when given.
- [ ] **Step 2: Run** the IPC tests in WSL and `pnpm test` in `extension`. Expected: FAIL.
- [ ] **Step 3: Implement** the fields and pass `format` through `addStream`, `addPage`, `addVideo` and `sendFile`.
- [ ] **Step 4: Run** the same tests. Expected: PASS.
- [ ] **Step 5: Commit** `feat: send the chosen format from the extension`.

### Task 7: MP3 and video buttons

**Files:**
- Modify: `extension/src/lib/streams.ts`, `extension/src/content.ts:145-161,245-282`, `extension/src/popup.ts:90-140`, `extension/src/popup.css`
- Test: `extension/src/lib/streams.test.ts`

**Interfaces:**
- Consumes: `VideoFormat`
- Produces: `videoLabel(kind: StreamKind | "page", url: string): string`

- [ ] **Step 1: Write the failing test.** `videoLabel("hls", u) === "MP4"`, `videoLabel("page", u) === "MP4"`, `videoLabel("file", "https://a/b.webm?x=1") === "WEBM"`, and a file with no extension gives `"Video"`.
- [ ] **Step 2: Run** `pnpm test streams`. Expected: FAIL.
- [ ] **Step 3: Implement it,** then render the video and MP3 buttons in the hover button, each list row and the popup, as the Extension page shows. Both show "Sending…" and disable while a request runs.
- [ ] **Step 4: Run** `pnpm test && pnpm build`. Expected: PASS.
- [ ] **Step 5: End to end in WSL.** Save a site video as MP3 and as MP4, a plain file as MP3, and an HLS stream as MP3. Pause and resume one. Expected: the right file type each time.
- [ ] **Step 6: Commit** `feat(extension): choose MP3 or video for each detected video`.
