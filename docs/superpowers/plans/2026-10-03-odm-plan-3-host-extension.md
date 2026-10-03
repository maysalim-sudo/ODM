# ODM Plan 3: Native Host and Browser Extension Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clicking a download link in Chrome/Chromium or Firefox/Floorp hands it to ODM, with its cookies and Referer, and ODM finishes it. This adds the context menu, the extension popup and browser registration.

**Architecture:**
- `odm-core::ipc` defines the wire protocol: the envelope, the payloads, the error codes, and 4-byte little-endian length-prefixed framing (max 1 MiB).
- `odm-appcore` gains:
  - the command dispatcher (`ipc::dispatch`), plain async logic against `ManagerHandle`;
  - a named-pipe server (`pipe::serve`) that only the current user can open;
  - capture helpers: skip-hosts, header normalisation, the total-speed board, and reuse offers.
- A new `odm-host` crate:
  - a small synchronous binary that relays native-messaging frames from stdin to the pipe and writes every reply to stdout;
  - it starts ODM with `--background` when needed;
  - it also performs `--register` / `--unregister`.
- `extension/` is a TypeScript MV3 extension built by Vite into `dist/chromium` and `dist/firefox`. Logic lives in pure modules tested with vitest; `background.ts`, `content.ts` and `popup.ts` only wire browser events to those modules.

**Tech Stack:**
- Rust 1.98 (MSVC), tokio (`net::windows::named_pipe`), serde_json, windows-sys, winreg.
- Tauri 2 (existing).
- Svelte 5 (existing UI).
- TypeScript 6, Vite 8, vitest 5, `@types/chrome` for the extension.

**Spec:** `docs/superpowers/specs/2026-09-29-odm-design.md`, sections 2, 4.2, 5.5 (link refresh), 8 (everything except video sniffing, which is Plan 4), 9 (Refresh URL), 10 and 11 (host test).
**Carry-over:** `docs/superpowers/plans/2026-10-03-odm-plan-2-carryover.md`. Its "Do in Plan 3" section is covered by Tasks 1, 2, 3, 8 and 9.

## Decisions beyond the spec (binding for this plan)

1. **Capture filtering happens in the app, not the extension.**
   - The app checks capture on/off, `min_capture_bytes` and skip-hosts on every captured `ADD_DOWNLOAD`, using its current settings, and replies `ok:false, code "skipped"` so that the browser keeps the download.
   - The extension filters only non-http(s) URLs and Alt.
   - `PING` returns `{version}` only.
   - Cost if wrong: when ODM is closed and capture is off, a browser download still starts ODM in the tray.
2. **`ADD_DOWNLOAD` gains two optional fields:**
   - `source`: `"capture"` (default) or `"menu"`. Menu requests skip the capture filters.
   - `expires_at_ms`: the epoch ms after which the app must refuse with `"expired"`.
   - The extension sets `expires_at_ms = now + 4500` and waits 5000 ms. An app that answers after the extension has given up has therefore always refused, so no duplicate download is possible.
3. **The host starts ODM only for `ADD_DOWNLOAD`, `ADD_STREAM` and `SHOW_WINDOW`.** For `PING`, `GET_SUMMARY`, `PAUSE_ALL` and `RESUME_ALL` it replies `app_not_running`, so that opening the popup never launches ODM.
4. **`ADD_STREAM`** is parsed and answered `ok:false, code "not_supported"` until Plan 4.
5. **Two host manifests** are written next to `odm-host.exe`: `com.odm.host.chromium.json` (`allowed_origins`) and `com.odm.host.firefox.json` (`allowed_extensions`).
6. **Registration state:** `HKCU\Software\ODM` holds the `HostPath` and `AppPath` values. The host reads `AppPath` to start ODM.
7. **Test-only environment overrides:** `ODM_PIPE_NAME` (pipe name) and `ODM_APP_PATH` (the program the host starts). The app honours `ODM_PIPE_NAME` too.
8. **Reuse offer (spec 5.5).** When an `ADD_DOWNLOAD` names a file that matches a Paused or Failed download:
   - The app replies `ok:true` with the existing id.
   - It stores an offer and shows the window with "Continue the earlier download?". "Continue" refreshes the old download's URL and headers and resumes it. "Download again", or closing the dialog, adds a new download.
9. **Mark of the Web.** Completed files get `:Zone.Identifier` (`ZoneId=3`) with redacted URLs (no query, no userinfo). It is best-effort; on FAT/exFAT it only logs a warning.

## Global Constraints

- **Identity values** come only from `odm_core::ids` (Rust) and `extension/src/lib/ids.ts` (TS). The values: host name `com.odm.host`; Firefox ID `odm@odm.local`; pipe `\\.\pipe\odm-<windows-username>`; the Chromium ID is read from `extension/chromium-id.txt`.
- **Crate direction:** `odm-core` ← `odm-store` ← `odm-engine` ← `odm-appcore` ← `app/src-tauri`. `odm-host` depends on `odm-core` only, never on tokio or the engine.
- **Framing:** 4-byte little-endian length, then UTF-8 JSON, at most 1 048 576 bytes, in both directions and on both links (stdio and pipe).
- **Envelope:** `{ id, cmd, payload }` → `{ id, ok, payload?, error?: {code, message} }`. The host and the app **always** reply, and the reply echoes the request `id` (or `null` when the request could not be parsed).
- **The pipe** accepts only the current user (DACL with only the user's SID), rejects remote clients, and is created with `first_pipe_instance(true)`.
- **Privacy:** cookie and `Authorization` values are never logged. URLs are logged only through `odm_core::redact_url`. The host writes nothing to stdout except reply frames.
- **Browser downloads** are cancelled only after ODM replies `ok:true`. On any failure or a 5 s timeout, the browser download continues untouched.
- **Nothing is sent to ODM automatically** except a download the browser itself started.
- **UI copy** is sentence case, plain words, no exclamation marks.
- **Git:** single branch `master`. One commit per task; the controller pushes after each task's review passes.
- **Commands** run from the repo root in Git Bash with `export PATH="$HOME/.cargo/bin:$PATH"`. App front-end commands run from `app/`, extension commands from `extension/`.
- **Rust gates:** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -- --test-threads=4`.
- **App gates:** `pnpm check` (0/0), `pnpm test`, `pnpm build`.
- **Extension gates:** `pnpm check`, `pnpm test`, `pnpm build`.
- Win32, Tauri and browser API code in this plan was written from documentation but never compiled. Minimal fixes are expected; the behaviour and the Interfaces blocks are binding.

## Review Focus

1. **ODM cold-starts slower than the extension waits.** The browser keeps its download and ODM must not add a second copy. *Test: Task 4 `expired_capture_is_refused_and_not_added`; Task 10 `only_ok_true_cancels_the_browser_download`.*
2. **Garbage on stdin or the pipe** (malformed JSON, a frame over 1 MiB, a truncated frame, an unknown command, wrongly typed payload fields). Each gets a typed error reply, and the process keeps serving. *Tests: Task 1 framing tests; Task 4 `bad_payload_is_a_typed_error`; Task 5 `server_survives_garbage`; Task 6 `host_replies_to_garbage_and_continues`.*
3. **ODM not installed or not registered** (no `AppPath`). The host replies `app_start_failed` at once instead of hanging, and the browser continues. *Test: Task 6 `missing_app_path_replies_start_failed`.*
4. **A hostile site's filename** (bidi override U+202E, zero-width characters, `..\`, reserved names). It is sanitised before it reaches disk. *Tests: Task 1 `format_chars_are_stripped`; Task 4 `hostile_filename_is_sanitized`.*
5. **Skip-host entries typed loosely** (`https://WWW.Example.com:8080/x`, upper case, a trailing dot). They still match `example.com` and its subdomains, and never match `notexample.com`. *Test: Task 3 `skip_hosts_match_loosely_typed_entries`.*

---

### Task 1: `odm-core`: IPC protocol, framing and filename hardening

**Files:**
- Create: `crates/odm-core/src/ipc.rs`
- Modify: `crates/odm-core/src/lib.rs` (add `pub mod ipc;`), `crates/odm-core/src/ids.rs`, `crates/odm-core/src/filename.rs`, `crates/odm-core/Cargo.toml` (add `serde_json`)
- Test: unit tests inside `ipc.rs`, `ids.rs`, `filename.rs`

**Interfaces:**
- Produces (`odm_core::ipc`):
  ```rust
  pub const MAX_FRAME: usize = 1 << 20;
  pub mod cmd { pub const PING: &str = "PING"; ADD_DOWNLOAD, ADD_STREAM, GET_SUMMARY, PAUSE_ALL, RESUME_ALL, SHOW_WINDOW /* same pattern */ }
  pub mod code { pub const BAD_MESSAGE: &str = "bad_message"; TOO_LARGE="too_large", UNKNOWN_COMMAND="unknown_command",
                 BAD_PAYLOAD="bad_payload", SKIPPED="skipped", EXPIRED="expired", ADD_FAILED="add_failed",
                 NOT_SUPPORTED="not_supported", APP_NOT_RUNNING="app_not_running", APP_START_FAILED="app_start_failed",
                 PIPE_ERROR="pipe_error", TIMEOUT="timeout", INTERNAL="internal" }
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct Request { #[serde(default)] pub id: Option<String>, pub cmd: String, #[serde(default)] pub payload: serde_json::Value }
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct Response { pub id: Option<String>, pub ok: bool,
      #[serde(default, skip_serializing_if = "Option::is_none")] pub payload: Option<serde_json::Value>,
      #[serde(default, skip_serializing_if = "Option::is_none")] pub error: Option<IpcError> }
  pub struct IpcError { pub code: String, pub message: String }            // Serialize, Deserialize, Debug, Clone, PartialEq
  impl Response { pub fn ok(id: Option<String>, payload: serde_json::Value) -> Self;
                  pub fn err(id: Option<String>, code: &str, message: impl Into<String>) -> Self;
                  pub fn to_bytes(&self) -> Vec<u8> /* serde_json::to_vec, never fails */ }
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)] #[serde(rename_all = "snake_case")]
  pub enum Source { #[default] Capture, Menu }
  #[derive(Debug, Clone, PartialEq, Deserialize)]
  pub struct AddDownload { pub url: String, #[serde(default)] pub filename: Option<String>, #[serde(default)] pub referrer: Option<String>,
      #[serde(default)] pub headers: std::collections::BTreeMap<String, String>, #[serde(default)] pub cookies: Option<String>,
      #[serde(default)] pub filesize: Option<u64>, #[serde(default)] pub mime: Option<String>,
      #[serde(default)] pub source: Source, #[serde(default)] pub expires_at_ms: Option<u64> }
  #[derive(Debug, Clone, PartialEq, Deserialize)]
  pub struct AddStream { pub page_url: String, #[serde(default)] pub page_title: Option<String>, pub playlist_url: String,
      #[serde(default)] pub headers: std::collections::BTreeMap<String, String>, #[serde(default)] pub cookies: Option<String> }
  #[derive(Serialize)] pub struct PingReply { pub version: String }
  #[derive(Serialize)] pub struct Summary { pub active: u64, pub queued: u64, pub total_speed_bps: u64 }
  #[derive(Serialize)] pub struct Added { pub download_id: String }
  /// Commands for which the host may start ODM when the pipe is missing.
  pub fn starts_app(cmd: &str) -> bool;                                    // ADD_DOWNLOAD | ADD_STREAM | SHOW_WINDOW
  #[derive(Debug)] pub enum FrameError { Eof, TooLarge(usize), Truncated, Io(std::io::Error) }
  pub fn frame_len(header: [u8; 4]) -> Result<usize, FrameError>;          // TooLarge when > MAX_FRAME
  pub fn read_frame<R: std::io::Read>(r: &mut R) -> Result<Vec<u8>, FrameError>;
  pub fn write_frame<W: std::io::Write>(w: &mut W, body: &[u8]) -> std::io::Result<()>; // InvalidInput if > MAX_FRAME; flushes
  pub fn skip_bytes<R: std::io::Read>(r: &mut R, n: usize) -> std::io::Result<()>;
  ```
- Produces (`odm_core::ids`):
  - `pub fn chromium_ext_id() -> &'static str`: `include_str!("../../../extension/chromium-id.txt").trim()`. Task 7 creates that file, so until Task 7 this function is **not** added. Task 7 adds it.
  - `pub fn pipe_name_for(override_name: Option<String>, user: Option<String>) -> String`: returns the override when it is non-empty, otherwise `pipe_name(user or "user")`.
- Produces (`odm_core::filename::sanitize`): also strips Unicode format characters. Same signature.

**Read semantics:**
- `read_frame` returns `Eof` only when the stream ends before the first header byte.
- It returns `Truncated` when the stream ends inside the header or the body.
- On `TooLarge(n)` it has consumed only the 4 header bytes. The caller may then `skip_bytes(n)` to stay in sync.

- [ ] **Step 1: Write the failing tests** in `ipc.rs`:

```rust
#[test] fn frame_round_trip() {
    let mut buf = Vec::new();
    write_frame(&mut buf, br#"{"cmd":"PING"}"#).unwrap();
    assert_eq!(&buf[..4], &14u32.to_le_bytes());
    assert_eq!(read_frame(&mut &buf[..]).unwrap(), br#"{"cmd":"PING"}"#);
}
#[test] fn clean_eof_vs_truncated() {
    assert!(matches!(read_frame(&mut &b""[..]), Err(FrameError::Eof)));
    assert!(matches!(read_frame(&mut &[5u8, 0][..]), Err(FrameError::Truncated)));
    assert!(matches!(read_frame(&mut &[5u8, 0, 0, 0, b'{'][..]), Err(FrameError::Truncated)));
}
#[test] fn oversize_frame_is_reported_and_skippable() {
    let n = MAX_FRAME + 1;
    let mut data = (n as u32).to_le_bytes().to_vec();
    data.extend(std::iter::repeat_n(b' ', n));
    write_frame(&mut data, b"{}").unwrap();                       // a valid frame follows
    let mut r = &data[..];
    match read_frame(&mut r) { Err(FrameError::TooLarge(m)) => { assert_eq!(m, n); skip_bytes(&mut r, m).unwrap(); } other => panic!("{other:?}") }
    assert_eq!(read_frame(&mut r).unwrap(), b"{}");
}
#[test] fn exact_limit_is_accepted() {
    let mut buf = Vec::new();
    write_frame(&mut buf, &vec![b' '; MAX_FRAME]).unwrap();
    assert_eq!(read_frame(&mut &buf[..]).unwrap().len(), MAX_FRAME);
    assert!(write_frame(&mut Vec::new(), &vec![b' '; MAX_FRAME + 1]).is_err());
}
#[test] fn request_tolerates_missing_id_and_payload() {
    let r: Request = serde_json::from_str(r#"{"cmd":"PING"}"#).unwrap();
    assert_eq!((r.id, r.cmd.as_str(), r.payload), (None, "PING", serde_json::Value::Null));
}
#[test] fn response_shapes() {
    let ok = serde_json::to_value(Response::ok(Some("1".into()), serde_json::json!({"version":"0.1.0"}))).unwrap();
    assert_eq!(ok, serde_json::json!({"id":"1","ok":true,"payload":{"version":"0.1.0"}}));
    let e = serde_json::to_value(Response::err(None, code::SKIPPED, "capture is off")).unwrap();
    assert_eq!(e, serde_json::json!({"id":null,"ok":false,"error":{"code":"skipped","message":"capture is off"}}));
}
#[test] fn add_download_defaults() {
    let a: AddDownload = serde_json::from_str(r#"{"url":"https://x/a.zip"}"#).unwrap();
    assert_eq!((a.source, a.headers.len(), a.filesize, a.expires_at_ms), (Source::Capture, 0, None, None));
    let m: AddDownload = serde_json::from_str(r#"{"url":"u","source":"menu","filesize":5}"#).unwrap();
    assert_eq!((m.source, m.filesize), (Source::Menu, Some(5)));
    assert!(serde_json::from_value::<AddDownload>(serde_json::json!({"url": 5})).is_err());
}
#[test] fn only_add_and_show_start_the_app() {
    for c in [cmd::ADD_DOWNLOAD, cmd::ADD_STREAM, cmd::SHOW_WINDOW] { assert!(starts_app(c)); }
    for c in [cmd::PING, cmd::GET_SUMMARY, cmd::PAUSE_ALL, cmd::RESUME_ALL, "NOPE"] { assert!(!starts_app(c)); }
}
```

In `ids.rs`:
```rust
#[test] fn pipe_name_override_wins_when_non_empty() {
    assert_eq!(super::pipe_name_for(Some(r"\\.\pipe\t1".into()), Some("u".into())), r"\\.\pipe\t1");
    assert_eq!(super::pipe_name_for(Some(String::new()), Some("Torinou".into())), r"\\.\pipe\odm-Torinou");
    assert_eq!(super::pipe_name_for(None, None), r"\\.\pipe\odm-user");
}
```

In `filename.rs`:
```rust
#[test] fn format_chars_are_stripped() {
    assert_eq!(sanitize("evil\u{202E}gpj.exe"), "evilgpj.exe");
    assert_eq!(sanitize("a\u{200B}b\u{200D}c\u{FEFF}.txt"), "abc.txt");
    assert_eq!(sanitize("\u{2066}x\u{2069}.pdf"), "x.pdf");
    assert_eq!(sanitize("caf\u{00E9}.txt"), "caf\u{00E9}.txt"); // ordinary letters stay
}
```

- [ ] **Step 2: Run** `cargo test -p odm-core`. Expected: compile errors, because `ipc` and `pipe_name_for` don't exist, and `format_chars_are_stripped` fails.

- [ ] **Step 3: Implement**
  - `ipc.rs` per the Interfaces block.
  - `pipe_name_for`.
  - In `sanitize`, before the existing rules, drop these characters: U+00AD, U+061C, U+180E, U+200B–U+200F, U+202A–U+202E, U+2060–U+2064, U+2066–U+2069, U+FEFF.

- [ ] **Step 4: Run** `cargo test -p odm-core`. Expected: all pass.

- [ ] **Step 5: Commit**
  - Message: `feat(core): IPC protocol and framing; strip format characters from file names`

---

### Task 2: Store and engine: state counts, link refresh, Mark of the Web

**Files:**
- Modify: `crates/odm-store/src/repo.rs`, `crates/odm-store/src/lib.rs`, `crates/odm-engine/src/manager.rs`, `crates/odm-engine/src/download.rs` (`finish`)
- Test: `crates/odm-store/tests/store.rs`, `crates/odm-engine/tests/manager.rs`, `crates/odm-engine/tests/download.rs`

**Interfaces:**
- Produces (store):
  - `Store::count_by_state(&self) -> Result<Vec<(DownloadState, u64)>, StoreError>`: `SELECT state, COUNT(*) FROM downloads GROUP BY state`. Unknown state strings are skipped.
  - `Store::set_url(&self, id: &str, url: String) -> Result<(), StoreError>`: sets `url`, sets `final_url = NULL`, and keeps `etag`/`last_modified`.
- Produces (engine):
  - `ManagerHandle::state_counts(&self) -> Result<HashMap<DownloadState, u64>, OdmError>`, read straight from the store.
  - `ManagerHandle::active_count` is re-implemented as the sum of the counts whose state `is_active()`. Same signature.
  - `ManagerHandle::refresh_url(&self, id: &str, url: String, headers: Option<Vec<(String, String)>>) -> Result<(), OdmError>`:
    - It goes through the actor (`Cmd::RefreshUrl`) so it is ordered with resume.
    - It is accepted only when the state is Paused or Failed and the URL is http(s), checked with the same case-insensitive scheme check as `add`. Factor that check into `fn is_http_url(url: &str) -> bool`.
    - Otherwise it returns `OdmError` kind `Other`, with the message `"only a paused or failed download can get a new link"` or the scheme message.
    - It does **not** resume; the caller resumes.
  - `download::finish` writes Mark of the Web after the rename.
- Add a doc comment on `ManagerHandle::cancel`: "`delete_file` is ignored: cancel always deletes the part file and never touches a completed file." This is a carry-over minor.

**Mark of the Web content** (`#[cfg(windows)]`, written to `format!("{}:Zone.Identifier", final_path.display())`). The `ReferrerUrl` line is present only when `page_url` is `Some`:
```
[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=<redact_url(page_url)>\r\nHostUrl=<redact_url(url)>\r\n
```
If the write fails, log `tracing::warn!` without a URL and continue; the download still completes.

- [ ] **Step 1: Write the failing tests**
  - Store: `count_by_state_groups_rows`. Insert 2 Queued, 1 Paused and 1 Completed; the result as a map is `{Queued:2, Paused:1, Completed:1}`.
  - Store: `set_url_clears_final_url_keeps_validator`. Insert a record with `final_url`, `etag` set; after `set_url` the record has the new url, `final_url == None`, and the same `etag`.
  - Manager: `state_counts_and_active_count_agree`. Add 2 downloads with `start_paused: true`; `state_counts()[Paused] == 2` and `active_count() == 0`.
  - Manager: `refresh_url_resumes_from_partial_data`:
    - Server: `TestServer::start(8 * MB, Faults { bytes_per_sec: Some(2 * MB), ..Default::default() })`.
    - Add `/file`, wait until at least 1 MB is written, then pause and wait for Paused.
    - Call `refresh_url(id, s.url("/file?token=2"), None)` and then `resume`, and wait for Completed.
    - The file equals `body(8 * MB)`, and the stored `url` ends with `?token=2`.
    - The data was reused, not restarted. Record `paused_bytes` from `manager.get(id)` before the refresh, subscribe to events before `resume`, and assert that the first `Progress` event for `id` after the resume has `bytes >= paused_bytes`.
  - Manager: `refresh_url_refuses_active_and_bad_scheme`. On a running throttled download, `refresh_url` returns Err. On a paused one, an `ftp://x` URL returns Err.
  - Download (windows only): `completed_file_gets_mark_of_the_web`:
    - Download `s.url("/file?token=secret")` with `page_url = Some("https://site.example/page?q=1")`.
    - Read `<path>:Zone.Identifier`. It contains `ZoneId=3`, `HostUrl=http://127.0.0.1:` and `ReferrerUrl=https://site.example/page`, and does not contain `secret` or `q=1`.
- [ ] **Step 2: Run** `cargo test -p odm-store -p odm-engine`. Expected: these tests fail or don't compile.
- [ ] **Step 3: Implement** per the Interfaces block. `refresh_url` with `headers: Some(h)` calls `set_headers(id, h)`.
- [ ] **Step 4: Run** `cargo test -p odm-store -p odm-engine` three times (the tests are timing-based). Expected: all pass, including the existing suites.
- [ ] **Step 5: Commit**
  - Message: `feat(engine): state counts, link refresh for paused/failed downloads, Mark of the Web`

---

### Task 3: `odm-appcore`: capture helpers, speed board and reuse offers

**Files:**
- Create: `crates/odm-appcore/src/capture.rs`, `crates/odm-appcore/src/summary.rs`, `crates/odm-appcore/src/offers.rs`
- Modify: `crates/odm-appcore/src/lib.rs` (modules), `crates/odm-appcore/src/settings.rs` (`clamp` normalises `skip_hosts`)
- Test: unit tests in each new module; `crates/odm-appcore/tests/settings.rs`

**Interfaces:**
- Produces (`odm_appcore::capture`):
  ```rust
  /// Lower-case host without scheme, userinfo, port, path, trailing dot or a leading "www.". None if empty/invalid.
  pub fn normalize_host(entry: &str) -> Option<String>;
  /// True if the URL's host equals an entry or is a subdomain of one. Entries are already normalised.
  pub fn host_skipped(url: &str, skip_hosts: &[String]) -> bool;
  /// Trim names and values; drop invalid header names, values containing CR or LF, and these names
  /// (case-insensitive): Host, Connection, Keep-Alive, Proxy-Connection, TE, Trailer, Transfer-Encoding,
  /// Upgrade, Content-Length, Range, If-Range, Accept-Encoding. Case-insensitive duplicates: the last one wins,
  /// at the position of the first. Output order is otherwise the input order.
  pub fn normalize_headers(h: Vec<(String, String)>) -> Vec<(String, String)>;
  /// Why a captured download should stay in the browser, or None to take it.
  pub fn skip_reason(s: &Settings, url: &str, filesize: Option<u64>) -> Option<&'static str>;
  ```
  The `skip_reason` texts, exactly:
  - `"capture is off"`
  - `"the file is smaller than the minimum capture size"` (only when the size is known and `< min_capture_bytes`)
  - `"this site is on the skip list"`
- Produces (`odm_appcore::summary`):
  ```rust
  #[derive(Default)] pub struct SpeedBoard { /* id -> speed_bps */ }
  impl SpeedBoard { pub fn apply(&mut self, ev: &EngineEvent); pub fn total(&self) -> u64; pub fn clear(&mut self); }
  /// Spawns a task that feeds every manager event into the board; on Lagged it clears the board.
  pub fn track(m: &ManagerHandle) -> Arc<Mutex<SpeedBoard>>;
  ```
  `apply` behaves as follows: `Progress` sets that id's speed. `State` with a state that is not `is_active()`, and `Removed`, delete the id. Other events change nothing.
- Produces (`odm_appcore::offers`):
  ```rust
  #[derive(Debug, Clone, PartialEq, Serialize)] pub struct OfferDto { pub offer_id: String, pub existing_id: String, pub filename: String }
  #[derive(Default)] pub struct Offers { /* Mutex<HashMap<offer_id, (OfferDto, AddRequest)>> */ }
  impl Offers {
      pub fn create(&self, existing_id: String, filename: String, req: AddRequest) -> OfferDto;   // offer_id = uuid simple
      pub fn list(&self) -> Vec<OfferDto>;                                                         // oldest first
      pub fn take(&self, offer_id: &str) -> Option<(OfferDto, AddRequest)>;
  }
  /// The newest Paused or Failed file download whose filename equals `name` (ASCII case-insensitive).
  pub async fn find_reusable(m: &ManagerHandle, name: &str) -> Result<Option<DownloadRecord>, OdmError>;
  ```
- `Settings::clamp` maps every `skip_hosts` entry through `normalize_host`, drops entries that are `None`, and removes duplicates while keeping the order. It adds no warning.

- [ ] **Step 1: Write the failing tests**
  - `skip_hosts_match_loosely_typed_entries` (Review Focus 5):
    ```rust
    let list: Vec<String> = ["https://WWW.Example.com:8080/x", "cdn.Test.org.", "  "].iter().filter_map(|e| normalize_host(e)).collect();
    assert_eq!(list, ["example.com", "cdn.test.org"]);
    assert!(host_skipped("https://example.com/a.zip", &list));
    assert!(host_skipped("https://dl.EXAMPLE.com/a.zip", &list));
    assert!(host_skipped("http://www.example.com/a", &list));
    assert!(!host_skipped("https://notexample.com/a", &list));
    assert!(!host_skipped("https://test.org/a", &list));
    assert!(!host_skipped("not a url", &list));
    ```
  - `normalize_headers_dedupes_and_drops_unsafe`:
    - Input: `[("Referer","a"),("cookie","x=1"),("Host","h"),("Cookie","x=2"),("Bad Name","v"),("X-Ok","line\r\nInjected: 1"),("User-Agent"," ua ")]`.
    - Result: `[("Referer","a"),("Cookie","x=2"),("User-Agent","ua")]`. The kept name is taken from the last duplicate.
  - `skip_reason_checks_in_order`:
    - Defaults: `None` for a 5 MB download.
    - `capture_enabled = false`: `Some("capture is off")`.
    - `filesize: Some(10)`: the min-size reason.
    - `filesize: None`: `None`.
    - Skip-listed host: the skip-list reason.
  - `speed_board_tracks_active_downloads`. Applying `Progress{id:a, speed 100}`, `Progress{id:b, speed 50}` and then `State{id:a, state:Paused}` gives `total() == 50`. Applying `Removed{b}` gives 0.
  - `offers_create_list_take`. Create two offers: `list()` returns both in creation order. `take(first)` returns its request, and a second `take` returns `None`.
  - `find_reusable_picks_newest_paused_or_failed`, an async test against a real manager on an in-memory store, as the existing appcore tests do:
    - Add `a.zip` paused twice, then a third `a.zip` that completes, from the test server.
    - `find_reusable(m, "A.ZIP")` returns the newer paused one.
    - `find_reusable(m, "b.zip")` returns `None`.
  - In `tests/settings.rs`: `clamp_normalizes_skip_hosts`. `["Example.com", "https://example.com/", "bad host!"]` becomes `["example.com"]`. `bad host!` is dropped because `normalize_host` rejects characters outside `[a-z0-9.-]`.
- [ ] **Step 2: Run** `cargo test -p odm-appcore`. Expected: compile failures.
- [ ] **Step 3: Implement** per the Interfaces block. Parse URLs with `reqwest::Url` (already a dependency through the engine).
- [ ] **Step 4: Run** `cargo test -p odm-appcore`. Expected: all pass.
- [ ] **Step 5: Commit**
  - Message: `feat(appcore): capture filters, header normalisation, speed board, reuse offers`

---

### Task 4: `odm-appcore`: the IPC command dispatcher

**Files:**
- Create: `crates/odm-appcore/src/ipc.rs`
- Modify: `crates/odm-appcore/src/lib.rs`
- Test: `crates/odm-appcore/tests/ipc.rs`

**Interfaces:**
- Consumes:
  - from Task 1: `odm_core::ipc::*`;
  - from Task 3: `capture::{skip_reason, normalize_headers}`, `summary::SpeedBoard`, `offers::{Offers, find_reusable}`;
  - from Task 2: `ManagerHandle::state_counts`.
- Produces:
  ```rust
  pub struct IpcContext {
      pub manager: ManagerHandle,
      pub settings: Arc<Mutex<Settings>>,
      pub speeds: Arc<Mutex<SpeedBoard>>,
      pub offers: Arc<Offers>,
      pub show_window: Box<dyn Fn() + Send + Sync>,
      pub on_offer: Box<dyn Fn(&OfferDto) + Send + Sync>,
  }
  pub async fn dispatch(ctx: &IpcContext, req: Request) -> Response;   // never panics; always echoes req.id
  pub fn now_ms() -> u64;                                               // SystemTime since UNIX_EPOCH
  ```

**Behaviour per command:**
- `PING` → `{version: env!("CARGO_PKG_VERSION")}`.
- `GET_SUMMARY` → `Summary`:
  - `active` = the sum of the counts whose state `is_active()`;
  - `queued` = the Queued count;
  - `total_speed_bps` = `speeds.total()`.
- `PAUSE_ALL` / `RESUME_ALL` → call the manager and reply `{}`.
- `SHOW_WINDOW` → call `show_window()` and reply `{}`.
- `ADD_STREAM` → if the payload parses as `AddStream`, reply `not_supported` with the message `"Video streams arrive in a later version of ODM."`; otherwise reply `bad_payload`.
- `ADD_DOWNLOAD`, in this order:
  1. Parse `AddDownload`. On failure, reply `bad_payload` with the serde error text. It holds no header values, because serde reports types and positions only.
  2. If `expires_at_ms` is `Some(t)` and `now_ms() > t`, reply `expired` with `"ODM answered too late; the browser kept the download."`.
  3. If `source == Capture` and `skip_reason(...)` is `Some(r)`, reply `skipped` with message `r`.
  4. Build the headers:
     - start from `payload.headers` in map order;
     - add `("Referer", referrer)` if a referrer is given and no `referer` header is present;
     - add `("Cookie", cookies)` if the cookies are non-empty;
     - pass the result through `normalize_headers`.
  5. Build the `AddRequest`:
     - `filename` = the payload filename's basename, taken after the last `/` or `\`;
     - `page_url = referrer`;
     - `headers` from step 4.
     The engine sanitises the name.
  6. Find a reuse candidate:
     - name = `filename::sanitize(basename)` if given, else `filename::from_url(&url)`;
     - if that name exists and `find_reusable` returns `Some(rec)`: `offers.create(rec.id, rec.filename, req)`, call `on_offer`, call `show_window()`, and reply `Added{download_id: rec.id}`.
  7. Otherwise call `manager.add(req)`. On `Ok(id)` reply `Added{download_id}`. On `Err(e)` reply `add_failed` with `user_message(&e)`.
- An unknown `cmd` → `unknown_command` with `"Unknown command: <cmd>"` (the cmd is truncated to 64 characters).
- Logging: `tracing::info!` with the cmd, `redact_url(url)` and the header **names** only.

- [ ] **Step 1: Write the failing tests** in `tests/ipc.rs`.
  - Helper `ctx()`: a manager on an in-memory store with the download dir in a tempdir, default settings, and `show_window` / `on_offer` closures that count calls through `Arc<AtomicUsize>`. Helper `req(cmd, payload)` builds `Request{id: Some("r1")}`.
  - `ping_returns_version`: ok, `payload.version == env!("CARGO_PKG_VERSION")`, and `id == Some("r1")`.
  - `add_download_capture_adds_with_cookies_and_referer`:
    - Test server with an 8 MB `/file`. Payload: `{url, filename:"C:\\Users\\x\\Downloads\\movie.mp4", referrer:"https://site/p", cookies:"sid=abc", filesize: 8388608, expires_at_ms: now+60000}`.
    - Reply ok with `download_id`.
    - `manager.get(id)`: filename `movie.mp4` and `page_url == Some("https://site/p")`.
    - The store's headers contain `("Cookie","sid=abc")` and `("Referer","https://site/p")`.
  - `expired_capture_is_refused_and_not_added` (Review Focus 1): `expires_at_ms: 1` gives `ok:false`, code `expired`, and `manager.list()` is empty.
  - `capture_filters_apply_but_menu_bypasses_them`:
    - With `capture_enabled = false`: a capture gives `skipped` / `"capture is off"`; the same payload with `"source":"menu"` gives ok.
    - With `filesize: 10`: a capture is skipped.
  - `bad_payload_is_a_typed_error` (Review Focus 2): `{"url": 5}` gives `bad_payload`; a missing payload gives `bad_payload`; `cmd "FOO"` gives `unknown_command`; none of them panic.
  - `hostile_filename_is_sanitized` (Review Focus 4): filename `"..\\..\\evil\u{202E}txt.exe"` gives a stored filename without `\`, `/`, `..` or `\u{202E}`.
  - `summary_counts_and_speed`: two paused downloads plus `speeds.lock().apply(Progress{speed 1000})` give `{active:0, queued:0, total_speed_bps:1000}`, and the paused ones are not counted as queued.
  - `pause_all_resume_all_show_window`: all three reply ok, and the show counter is 1.
  - `add_stream_is_not_supported_yet`: a valid payload gives `not_supported`; `{}` gives `bad_payload`.
  - `matching_paused_download_creates_an_offer`:
    - Add `movie.mp4` paused through the manager.
    - `ADD_DOWNLOAD{filename:"movie.mp4", source:"menu"}` replies `download_id == existing id`.
    - `offers.list().len() == 1`, `on_offer` and `show_window` were each called once, and `manager.list().len()` is still 1.
- [ ] **Step 2: Run** `cargo test -p odm-appcore --test ipc`. Expected: compile failure.
- [ ] **Step 3: Implement** `ipc.rs` per the Behaviour list.
- [ ] **Step 4: Run** `cargo test -p odm-appcore` twice. Expected: all pass.
- [ ] **Step 5: Commit**
  - Message: `feat(appcore): IPC command dispatcher`

---

### Task 5: `odm-appcore`: named-pipe server

**Files:**
- Create: `crates/odm-appcore/src/pipe.rs` (`#![cfg(windows)]` module; `lib.rs` declares it under `#[cfg(windows)]`)
- Modify: `crates/odm-appcore/Cargo.toml` (`windows-sys` with the features the security code needs: `Win32_Foundation`, `Win32_Security`, `Win32_Security_Authorization`, `Win32_System_Threading`), workspace `Cargo.toml` if features are declared there
- Test: `crates/odm-appcore/tests/pipe.rs` (`#![cfg(windows)]`)

**Interfaces:**
- Consumes: `ipc::{IpcContext, dispatch}` from Task 4; `odm_core::ipc::{frame_len, MAX_FRAME, Request, Response, code}`.
- Produces:
  ```rust
  /// The pipe name ODM listens on: ODM_PIPE_NAME if set and non-empty, else \\.\pipe\odm-<USERNAME>.
  pub fn pipe_name_from_env() -> String;            // uses odm_core::ids::pipe_name_for
  /// "D:P(A;;GA;;;<current user SID>)". Errors if the token can't be read.
  pub fn current_user_sddl() -> std::io::Result<String>;
  /// Binds the first instance before returning, so a squatted name is an immediate Err.
  /// Then serves clients concurrently until `stop` is cancelled.
  pub async fn serve(name: String, ctx: Arc<IpcContext>, stop: CancellationToken) -> std::io::Result<tokio::task::JoinHandle<()>>;
  ```

**Server behaviour:**
- Every instance is created with:
  - `ServerOptions::new().first_pipe_instance(<true for the first only>).reject_remote_clients(true)`;
  - the SDDL above, via `ConvertStringSecurityDescriptorToSecurityDescriptorW` and `create_with_security_attributes_raw`.
  Free the descriptor with `LocalFree` after creation.
- After each `connect().await`, create the next instance first, then spawn a client task.
- The client task loops:
  - Read 4 header bytes. A clean EOF ends the task.
  - `frame_len`: on `TooLarge`, write `Response::err(None, TOO_LARGE, "Message is larger than 1 MB.")` and close the connection.
  - Read the body. A truncated body ends the task.
  - `serde_json::from_slice::<Request>`: on failure, reply `BAD_MESSAGE` with `"Message is not valid JSON."` and continue.
  - Otherwise `dispatch`, then write the reply frame.
  - Each client task also stops when `stop` is cancelled.

- [ ] **Step 1: Write the failing tests** in `tests/pipe.rs`. Each test uses a unique name `format!(r"\\.\pipe\odm-test-{}", uuid)` and the Task 4 context helper (copy the helper; tests can't share across files without a `common` module, so create `tests/common/mod.rs` and move the Task 4 helper there).
  - `round_trip_ping`: connect with `ClientOptions::new().open(&name)`, write a framed `{"id":"a","cmd":"PING"}`, and read a framed reply with `id "a"` and `ok:true`.
  - `server_survives_garbage` (Review Focus 2):
    - On one connection, send `b"not json"`: the reply is `bad_message`. Then send a valid PING on the same connection: ok.
    - On a second connection, send a header announcing `MAX_FRAME + 1`: the reply is `too_large`, then EOF.
    - A third connection's PING still works.
  - `concurrent_clients`: 8 tasks each connect and PING, and all get ok.
  - `second_server_on_same_name_fails`: a second `serve(name.clone(), ...)` returns `Err`.
  - `sddl_names_current_user`: `current_user_sddl()` starts with `"D:P(A;;GA;;;S-1-5-"`.
  - `stop_ends_the_server`: cancel the token; within 2 s the join handle finishes, and a new connection attempt fails.
- [ ] **Step 2: Run** `cargo test -p odm-appcore --test pipe`. Expected: compile failure.
- [ ] **Step 3: Implement** `pipe.rs` per the Server behaviour.
- [ ] **Step 4: Run** `cargo test -p odm-appcore --test pipe` three times. Expected: all pass.
- [ ] **Step 5: Commit**
  - Message: `feat(appcore): named-pipe server limited to the current user`

---

### Task 6: `odm-host`: the stdio relay

**Files:**
- Create: `crates/odm-host/Cargo.toml`, `crates/odm-host/src/lib.rs`, `crates/odm-host/src/relay.rs`, `crates/odm-host/src/main.rs`
- Test: `crates/odm-host/tests/relay.rs`

**Cargo:**
- `[dependencies]`: `odm-core`, `serde_json`, `winreg = "0.55"` (or the newest 0.x that resolves).
- `[dev-dependencies]`: `tokio`, `uuid`.
- `[[bin]] name = "odm-host"`. No tokio in normal dependencies.

**Interfaces:**
- Produces:
  ```rust
  pub struct HostConfig { pub pipe_name: String, pub app_path: Option<PathBuf>, pub start_wait: Duration, pub reply_timeout: Duration }
  impl HostConfig {
      /// ODM_PIPE_NAME / USERNAME for the pipe; ODM_APP_PATH, else HKCU\Software\ODM\AppPath, for the app;
      /// start_wait 10 s, reply_timeout 10 s.
      pub fn from_env() -> Self;
  }
  /// Reads frames from `input` until EOF; writes exactly one reply frame per request frame.
  pub fn run<R: Read, W: Write>(input: &mut R, output: &mut W, cfg: &HostConfig) -> std::io::Result<()>;
  ```
- `main.rs`:
  - `--register` and `--unregister` are handled in Task 7. In this task they print `"not implemented yet"` to **stderr** and exit with code 2.
  - Any other arguments (Chrome passes the origin and `--parent-window=N`; Firefox passes the manifest path and extension id) run `run(stdin.lock(), stdout.lock(), &HostConfig::from_env())`.
  - It exits with code 0 on EOF.

**Relay behaviour, per frame:**
1. `read_frame`:
   - `Eof` → return Ok.
   - `TooLarge(n)` → `skip_bytes(n)`, reply `too_large` with id `null`, continue.
   - `Truncated` → reply `bad_message` with id `null`, return Ok.
2. Parse as `Request`. On failure, reply `bad_message` with id `null`. On success keep the `id` and `cmd`; the body is forwarded byte-for-byte.
3. Open the pipe as a file: `OpenOptions::new().read(true).write(true).open(&pipe_name)`.
   - Raw OS error 231 (`ERROR_PIPE_BUSY`): retry every 50 ms for up to 2 s.
   - Error 2 (not found):
     - if `!starts_app(cmd)` → reply `app_not_running` with `"ODM is not running."`;
     - else if `app_path` is `None` or the spawn fails → reply `app_start_failed` with `"ODM could not be started. Open ODM once to set up browser integration."`;
     - else spawn the app, then retry opening every 200 ms until `start_wait` has passed → reply `app_start_failed` if it never appears.
   - Other errors → reply `pipe_error`.
4. Spawn `app_path --background` with `creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB)` (0x8, 0x200, 0x0100_0000) and null stdio. If that spawn fails, retry once without `CREATE_BREAKAWAY_FROM_JOB`.
5. On a worker thread: write the frame, then read one reply frame. The main thread waits `reply_timeout` (`mpsc::recv_timeout`); on timeout, reply `timeout` with `"ODM did not answer in time."`.
   - A reply over `MAX_FRAME`, or one that isn't valid JSON → `pipe_error`.
   - Otherwise write the app's reply bytes unchanged.
6. Flush stdout after every reply. Never write anything else to stdout.

- [ ] **Step 1: Write the failing tests** in `tests/relay.rs`:
  - Each test spawns `env!("CARGO_BIN_EXE_odm-host")` with `ODM_PIPE_NAME` set to a unique test pipe, writes frames to its stdin, closes stdin, and reads every stdout frame with `odm_core::ipc::read_frame`.
  - The fake server is a tokio named-pipe server in the test. It replies `{"id":<same>,"ok":true,"payload":{"echo":<cmd>}}`.
  - Tests:
    - `forwards_and_returns_reply`: a PING gets `ok:true` with `payload.echo == "PING"`, and the process exits 0.
    - `host_replies_to_garbage_and_continues` (Review Focus 2):
      - The frames: `b"nope"`, then a frame announcing 2 MiB (with 2 MiB of bytes following), then a PING.
      - The replies, in order: `bad_message` (id null), `too_large` (id null), then the echo for PING. That is exactly 3 frames.
    - `no_app_for_ping_replies_not_running`: with no server, a PING gets `app_not_running`.
    - `missing_app_path_replies_start_failed` (Review Focus 3): no server, `ODM_APP_PATH` unset, and `HKCU\Software\ODM` absent. Run with the env var `ODM_APP_PATH=""`; `from_env` treats an empty value as "explicitly none" and skips the registry. `ADD_DOWNLOAD` gets `app_start_failed` within 1 s.
    - `starts_app_then_waits_for_pipe`:
      - `ODM_APP_PATH = %SystemRoot%\System32\whoami.exe`: it spawns and exits at once, so the spawn succeeds.
      - The test starts its fake server 1 s after writing `SHOW_WINDOW`.
      - The reply is the echo, and it arrives after at least 0.9 s.
    - `truncated_input_gets_a_reply`: stdin holds `[9,0,0,0,b'{']` then EOF. The single reply is `bad_message`, and the exit code is 0.
- [ ] **Step 2: Run** `cargo test -p odm-host`. Expected: compile failure.
- [ ] **Step 3: Implement** `relay.rs`, `lib.rs` and `main.rs` per the behaviour above.
- [ ] **Step 4: Run** `cargo test -p odm-host` three times. Expected: all pass.
- [ ] **Step 5: Commit**
  - Message: `feat(host): native messaging relay to the ODM pipe`

---

### Task 7: `odm-host --register` / `--unregister` and the fixed Chromium ID

**Files:**
- Create:
  - `extension/scripts/chromium-id.mjs` (exports `idFromKey(base64Spki)`)
  - `extension/scripts/make-key.mjs`
  - `extension/chromium-key.txt`, `extension/chromium-id.txt` (both generated by the script and committed)
  - `crates/odm-host/src/register.rs`
- Modify: `crates/odm-host/src/main.rs`, `crates/odm-host/src/lib.rs`, `crates/odm-core/src/ids.rs` (add `chromium_ext_id()`)
- Test: unit tests in `register.rs`; `crates/odm-host/tests/register.rs`

**Chromium key:**
- `make-key.mjs` uses `crypto.generateKeyPairSync("rsa", {modulusLength: 2048, publicKeyEncoding: {type: "spki", format: "der"}, privateKeyEncoding: {type: "pkcs8", format: "pem"}})`.
- It writes `chromium-key.txt` (base64 of the DER public key, one line) and `chromium-id.txt` (`idFromKey(...)` plus a newline).
- It refuses to overwrite either file unless `--force` is given. It never writes the private key anywhere.
- `idFromKey`: take the SHA-256 of the DER bytes, keep the first 32 hex characters, and map each hex digit `0`–`f` to `a`–`p`.
- Run it once in this task: `node extension/scripts/make-key.mjs`.

**Interfaces:**
- Produces:
  ```rust
  pub struct RegisterOptions {
      pub host_exe: PathBuf, pub app_exe: PathBuf, pub chromium_id: String,
      /// Prepended to every HKCU subkey path. "" in production; "Software\\ODM-test-<uuid>\\" in tests.
      pub hkcu_prefix: String,
  }
  pub const CHROMIUM_KEYS: [&str; 6] = [
      r"Software\Google\Chrome\NativeMessagingHosts", r"Software\Microsoft\Edge\NativeMessagingHosts",
      r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts", r"Software\Vivaldi\NativeMessagingHosts",
      r"Software\Opera Software\NativeMessagingHosts", r"Software\Chromium\NativeMessagingHosts" ];
  pub const MOZILLA_KEY: &str = r"Software\Mozilla\NativeMessagingHosts";   // Firefox and Floorp
  pub const ODM_KEY: &str = r"Software\ODM";
  pub fn chromium_manifest(host_exe: &Path, chromium_id: &str) -> serde_json::Value;
  pub fn firefox_manifest(host_exe: &Path) -> serde_json::Value;
  pub fn register(o: &RegisterOptions) -> std::io::Result<()>;
  pub fn unregister(o: &RegisterOptions) -> std::io::Result<()>;   // missing keys/files are not errors
  ```
- The manifests, exactly:
  - Chromium: `{"name":"com.odm.host","description":"ODM native messaging host","path":<host_exe>,"type":"stdio","allowed_origins":["chrome-extension://<id>/"]}`
  - Firefox: the same, with `"allowed_extensions":["odm@odm.local"]` in place of `allowed_origins`.
- `register`:
  - It writes `<host dir>\com.odm.host.chromium.json` and `<host dir>\com.odm.host.firefox.json`, pretty-printed.
  - For each Chromium key, it sets `<prefix><key>\com.odm.host` (Default) to the Chromium manifest path. For the Mozilla key, it sets the same to the Firefox manifest path.
  - It sets the `<prefix>Software\ODM` values `HostPath` and `AppPath`.
- `unregister`: deletes the `com.odm.host` subkeys, the `<prefix>Software\ODM` key and both manifest files. It never deletes the parent keys.
- `main.rs`:
  - `--register --app <path>`: `host_exe = current_exe()` and the production prefix `""`. Prints `"Registered ODM for Chrome, Edge, Brave, Vivaldi, Opera, Chromium, Firefox and Floorp."` to stdout and exits with code 0. Prints the error to stderr and exits 1 on failure. A missing `--app` exits 2 with a usage line.
  - `--unregister`: the same pattern.
- `odm_core::ids::chromium_ext_id()`: `include_str!("../../../extension/chromium-id.txt").trim()`, with a unit test that it is 32 characters in `a..=p`.

- [ ] **Step 1: Write the failing tests**
  - `register.rs` unit test `manifests_match_spec`: compare both manifests against the exact JSON values above for `host_exe = C:\x\odm-host.exe` and `id = "abcdefghijklmnopabcdefghijklmnop"`.
  - `tests/register.rs` `register_then_unregister_round_trip`:
    - Use a tempdir as the host dir (`host_exe = tmp\odm-host.exe`; the file need not exist) and the prefix `Software\ODM-test-<uuid>\`.
    - After `register`: both files exist and parse. Each of the 7 keys' default value equals the right manifest path. `HostPath` and `AppPath` read back.
    - After `unregister`: the files are gone, the `com.odm.host` subkeys and the ODM key are gone, and the parent `...\NativeMessagingHosts` keys still exist.
    - Finally, delete the whole `Software\ODM-test-<uuid>` tree in a drop guard, even if an assertion failed.
  - `ids.rs`: `chromium_id_shape`.
  - Key/ID consistency: the permanent test lives in the extension's vitest suite (Task 10). In this task, verify it once by running `node -e "import('./extension/scripts/chromium-id.mjs').then(m=>console.log(m.idFromKey(require('fs').readFileSync('extension/chromium-key.txt','utf8').trim())))"` and comparing the output with `chromium-id.txt`.
- [ ] **Step 2: Run** `cargo test -p odm-host -p odm-core`. Expected: failures (missing functions or the missing file).
- [ ] **Step 3: Implement**: run `make-key.mjs`, then implement `register.rs`, `main.rs` and `chromium_ext_id`.
- [ ] **Step 4: Run** `cargo test -p odm-host -p odm-core`. Expected: all pass. Also check that no `HKCU\Software\ODM-test-*` keys remain: `reg query HKCU\Software /f ODM-test /k` prints "0 match(es)".
- [ ] **Step 5: Commit**
  - Message: `feat(host): register the native host for Chromium browsers, Firefox and Floorp`

---

### Task 8: App wiring: pipe server, background start, auto-registration, offer and refresh commands

**Files:**
- Create: `crates/odm-appcore/src/registration.rs`
- Modify:
  - `crates/odm-appcore/src/dto.rs`: `UiEvent::ReuseOffer { offer: OfferDto }`, serialised as `{"type":"reuse_offer","offer":{...}}`
  - `crates/odm-appcore/src/api.rs`: `refresh_url`, `resolve_offer`
  - `crates/odm-appcore/Cargo.toml`: winreg under `[target.'cfg(windows)'.dependencies]`
  - `app/src-tauri/src/lib.rs`, `app/src-tauri/src/shell.rs`, `app/src-tauri/src/commands.rs`
- Test: `crates/odm-appcore/tests/api.rs`; unit tests in `registration.rs`

**Interfaces:**
- Produces (`odm_appcore::registration`):
  ```rust
  /// True if HostPath or AppPath is missing or differs (case-insensitive, after canonical \ separators) from these paths.
  pub fn registration_needed(host_exe: &Path, app_exe: &Path, read: impl Fn(&str) -> Option<String>) -> bool;
  /// Windows: reads HKCU\Software\ODM; if needed, runs `<host_exe> --register --app <app_exe>` with CREATE_NO_WINDOW
  /// (0x0800_0000), waits up to 10 s, logs the outcome. Missing host exe → warn "browser integration needs odm-host.exe next to ODM".
  pub fn ensure_registered(host_exe: &Path, app_exe: &Path);
  ```
- Produces (`odm_appcore::api`):
  ```rust
  /// Gives a paused/failed download a new link (stored headers kept) and resumes it.
  pub async fn refresh_url(m: &ManagerHandle, id: &str, url: &str) -> Result<(), String>;
  /// reuse = true: refresh_url(existing, req.url, Some(req.headers)) then resume. false: m.add(req).
  /// Unknown offer id → Err("This offer is no longer available.").
  pub async fn resolve_offer(m: &ManagerHandle, offers: &Offers, offer_id: &str, reuse: bool) -> Result<String, String>; // returns download id
  ```
- Produces (Tauri commands, added to `generate_handler!`):
  - `list_offers() -> Vec<OfferDto>`
  - `resolve_offer(offer_id: String, reuse: bool) -> Result<String, String>`
  - `refresh_url(id: String, url: String) -> Result<(), String>`
- Changes to `AppState`:
  - `settings` becomes `Arc<Mutex<Settings>>`, and every existing use is updated.
  - New fields: `offers: Arc<Offers>` and `ipc_stop: CancellationToken`.

**Startup and shutdown wiring (`lib.rs` / `shell.rs`):**
- `run()` computes `let background = std::env::args().any(|a| a == "--background");` and passes it to `shell::setup`. The window is shown only when `!(start_minimized || background)`.
- The single-instance callback calls `show_main` only when argv lacks `--background`.
- In `setup`, after the manager starts:
  1. `let speeds = summary::track(&manager);`
  2. Build the `IpcContext`:
     - `show_window` = a closure that clones the `AppHandle` and calls `shell::show_main` on the main thread (`run_on_main_thread`);
     - `on_offer` = emit `UiEvent::ReuseOffer` on `odm://event`.
  3. Call `pipe::serve(pipe_name_from_env(), ctx, ipc_stop.clone())` inside `tauri::async_runtime::block_on`. On `Err`, log `error!("browser integration is unavailable: {e}")` and keep running.
  4. Spawn a thread running `registration::ensure_registered(current_exe dir join "odm-host.exe", current_exe)`.
- On `RunEvent::Exit`, call `ipc_stop.cancel()` before the manager shuts down.

- [ ] **Step 1: Write the failing tests**
  - `registration.rs` `registration_needed_cases`:
    - Both values equal, differing only in case or `/` vs `\` → false.
    - `HostPath` missing → true.
    - `AppPath` different → true.
  - `tests/api.rs` `refresh_url_api_resumes`: a paused download with a throttled server, then `api::refresh_url(m, id, new_url)` → the state becomes Downloading or Completed within 2 s.
  - `tests/api.rs` `resolve_offer_reuse_and_fresh`:
    - Offer A with reuse=true returns the existing id, and the list length stays 1.
    - Offer B with reuse=false returns a new id, and the list length becomes 2.
    - Unknown offer → Err with the exact text above.
  - `dto` test: `reuse_offer_event_json`. `serde_json::to_value(UiEvent::ReuseOffer{offer})` equals `{"type":"reuse_offer","offer":{"offer_id":"o","existing_id":"e","filename":"f.zip"}}`.
- [ ] **Step 2: Run** `cargo test -p odm-appcore`. Expected: compile failures.
- [ ] **Step 3: Implement** the appcore parts, then the Tauri wiring.
- [ ] **Step 4: Run the Rust gates, plus** `cargo build -p odm-host -p odm-app`.
  - Then run `ODM_DATA_DIR=$TEMP/odm-p3-check ODM_PIPE_NAME='\\.\pipe\odm-p3-check' ./target/debug/odm-app.exe --background` in the background.
  - Wait 5 s. Run `printf` a framed PING through `ODM_PIPE_NAME='\\.\pipe\odm-p3-check' ./target/debug/odm-host.exe`. Use a tiny Node one-liner to frame it: `node -e "const b=Buffer.from(JSON.stringify({id:'1',cmd:'PING'}));const h=Buffer.alloc(4);h.writeUInt32LE(b.length);process.stdout.write(Buffer.concat([h,b]))" | ODM_PIPE_NAME=... ./target/debug/odm-host.exe | od -c | head`.
  - Expected: the output contains `"ok":true` and `"version"`.
  - Kill `odm-app.exe`. Record the output in the report.
  - Note: this run also registers the debug host under the real HKCU. That is intended. It points at `target\debug`, and Task 12 relies on it.
- [ ] **Step 5: Commit**
  - Message: `feat(app): browser integration server, background start, auto-registration`

---

### Task 9: UI: reuse offer, Refresh URL, and the warning before opening a program

**Files:**
- Create: `app/src/components/ReuseDialog.svelte`, `app/src/components/RefreshUrlDialog.svelte`, `app/src/components/OpenProgramDialog.svelte`
- Modify: `app/src/lib/types.ts` (or wherever `UiEvent` and `Download` are typed), `app/src/lib/api.ts`, `app/src/lib/model.ts`, `app/src/lib/downloads.svelte.ts`, `app/src/components/DetailPanel.svelte`, `app/src/App.svelte`
- Test: `app/src/lib/model.test.ts`

**Interfaces:**
- Consumes: the Task 8 commands `list_offers`, `resolve_offer`, `refresh_url`, and the `reuse_offer` event.
- Produces:
  ```ts
  export type Offer = { offer_id: string; existing_id: string; filename: string };
  // UiEvent gains: | { type: "reuse_offer"; offer: Offer }
  export const listOffers = () => invoke<Offer[]>("list_offers");
  export const resolveOffer = (offerId: string, reuse: boolean) => invoke<string>("resolve_offer", { offerId, reuse });
  export const refreshUrl = (id: string, url: string) => invoke<void>("refresh_url", { id, url });
  // model.ts
  export function isRiskyFile(name: string): boolean;     // by extension, case-insensitive
  export function canRefresh(r: Row): boolean;            // state is "paused" or "failed"
  export function addOffer(list: Offer[], o: Offer): Offer[];   // ignores a duplicate offer_id
  ```
- Risky extensions: `exe msi bat cmd com scr ps1 vbs vbe js jse wsf wsh hta lnk reg cpl msc jar appx msix`.
- `applyEvent` ignores `reuse_offer`: rows are unchanged and no reload is triggered. The store handles that event separately, before `applyEvent`.
- On `init()`, the store loads `listOffers()`. It keeps `offers: Offer[]` and shows the first one.

**Copy (exact):**
- **ReuseDialog** (on `Modal`):
  - Title: "Continue the earlier download?"
  - Text: "{filename} was partly downloaded before. ODM can continue it with the new link, or download it again from the start."
  - Buttons: "Download again" (secondary) and "Continue" (primary).
  - Closing or Escape counts as "Download again".
  - It has a `busy` guard. An error is shown inline as the command's message.
  - After a choice, the offer is removed and the next one, if any, is shown.
- **RefreshUrlDialog:**
  - Title: "Refresh the link".
  - Label: "New link".
  - Hint: "Paste a fresh link for the same file from the browser. ODM continues from where it stopped."
  - Buttons: "Cancel" and "Continue download". The second is disabled until the input starts with `http://` or `https://`, compared case-insensitively.
  - Errors are shown inline.
- **DetailPanel:** a "Refresh URL" button, shown when `canRefresh(row)`, opens RefreshUrlDialog. "Open file" on a risky file opens OpenProgramDialog first.
- **OpenProgramDialog:**
  - Title: "Open this program?"
  - Text: "{name} is a program. Programs from the internet can harm your computer. Only open it if you trust where it came from."
  - Buttons: "Cancel" and "Open".
- The Delete-key shortcut is blocked while any of these dialogs is open, matching the existing dialogs.

- [ ] **Step 1: Write the failing tests** in `model.test.ts`:
  - `isRiskyFile`:
    - `"setup.EXE"` → true; `"a.tar.gz"` → false; `"script.ps1"` → true; `"noext"` → false; `"movie.mp4"` → false.
    - `".exe"` → true: a name that is only an extension counts.
  - `canRefresh`: paused → true, failed → true, downloading → false, completed → false.
  - `addOffer` ignores a duplicate id.
  - `applyEvent` with `{type:"reuse_offer"}` returns the same rows and `reload: false`, matching the existing return shape.
- [ ] **Step 2: Run** `cd app && pnpm test`. Expected: failures.
- [ ] **Step 3: Implement** the model functions, the API wrappers, the store handling and the three dialogs.
- [ ] **Step 4: Run** `pnpm check` (0 errors, 0 warnings), `pnpm test` and `pnpm build`. Expected: all pass.
- [ ] **Step 5: Commit**
  - Message: `feat(ui): reuse offer, refresh link, warning before opening programs`

---

### Task 10: Extension foundation: pure modules, manifests, build

**Files:**
- Create:
  - `extension/package.json`, `extension/tsconfig.json`, `extension/vitest.config.ts`
  - `extension/scripts/build.mjs`
  - `extension/manifests/chromium.json`, `extension/manifests/firefox.json`
  - `extension/src/lib/ids.ts`, `protocol.ts`, `capture.ts`, `alt.ts`, `timeout.ts`, `format.ts`, each with a `*.test.ts` beside it
  - `extension/src/lib/chromium-id.test.ts`
- Modify: root `.gitignore` (add `extension/node_modules`, `extension/dist`)

**package.json:**
- `"type":"module"`, `"private":true`.
- Scripts:
  - `build`: `node scripts/build.mjs`
  - `test`: `vitest run`
  - `check`: `tsc --noEmit`
- devDependencies: `typescript ^6`, `vite ^8`, `vitest ^5`, `@types/chrome` (latest).

**Interfaces (all pure; no `chrome.*` access in `src/lib`):**
```ts
// ids.ts
export const HOST_NAME = "com.odm.host";
export const FIREFOX_ID = "odm@odm.local";
export const CAPTURE_TIMEOUT_MS = 5000;
export const APP_DEADLINE_MS = 4500;
export const ALT_GRACE_MS = 1500;
export const MENU_ID = "odm-download";
// protocol.ts
export type OdmResponse<T = unknown> = { id: string | null; ok: boolean; payload?: T; error?: { code: string; message: string } };
export type AddDownloadPayload = { url: string; filename?: string; referrer?: string; headers: Record<string, string>;
  cookies?: string; filesize?: number; mime?: string; source: "capture" | "menu"; expires_at_ms?: number };
export function makeRequest(cmd: string, payload: object = {}): { id: string; cmd: string; payload: object }; // id = crypto.randomUUID()
export function errorText(r: OdmResponse | null | undefined): string;  // "" when ok
// capture.ts
export type BrowserItem = { url: string; finalUrl?: string; referrer?: string; filename?: string; totalBytes?: number; mime?: string };
export type Cookie = { name: string; value: string };
export function isHttp(url: string): boolean;                            // http: or https:, case-insensitive
export function basename(path: string | undefined): string | undefined;  // after last / or \, undefined when empty
export function cookieHeader(cookies: Cookie[]): string | undefined;     // "a=1; b=2", undefined when none
export function captureUrl(item: BrowserItem): string;                    // finalUrl if http(s), else url
export function shouldOffer(item: BrowserItem, altActive: boolean): boolean; // isHttp(captureUrl) && !altActive
export function toCapturePayload(item: BrowserItem, cookies: Cookie[], userAgent: string, now: number): AddDownloadPayload;
export function toMenuPayload(url: string, pageUrl: string | undefined, cookies: Cookie[], userAgent: string): AddDownloadPayload;
/** The browser download may be cancelled only when this is true. */
export function shouldCancelBrowserDownload(r: OdmResponse | null): boolean;   // r?.ok === true
// alt.ts
export class AltTracker { down(now: number): void; up(now: number): void; active(now: number): boolean; } // held, or released < ALT_GRACE_MS ago
// timeout.ts
export function withTimeout<T>(p: Promise<T>, ms: number): Promise<T | null>;  // null on timeout or rejection
// format.ts
export function formatSpeed(bps: number): string;     // "0 B/s", "512 B/s", "1.5 KB/s", "3.4 MB/s" (1024 base, one decimal from KB)
export function statusLine(s: { active: number; queued: number; total_speed_bps: number }): string; // "2 active · 1 queued · 3.4 MB/s"
```
- `toCapturePayload`:
  - `url = captureUrl(item)`;
  - `filename = basename(item.filename)`;
  - `referrer = item.referrer || undefined`;
  - `headers = {"User-Agent": userAgent}`;
  - `cookies = cookieHeader(cookies)`;
  - `filesize = totalBytes > 0 ? totalBytes : undefined`;
  - `mime`;
  - `source "capture"`;
  - `expires_at_ms = now + APP_DEADLINE_MS`.
- `toMenuPayload`: `source "menu"`, no `expires_at_ms`, `referrer = pageUrl`.

**Manifests:**
- **Chromium** (`manifest_version 3`):
  - `name "ODM"`, `version "0.1.0"`, `description "Send downloads to ODM (Open Downloader Manager)."`
  - `permissions ["downloads","cookies","nativeMessaging","contextMenus","storage"]`, `host_permissions ["<all_urls>"]`
  - `background {"service_worker":"background.js"}`
  - `content_scripts [{"matches":["<all_urls>"],"js":["content.js"],"run_at":"document_start","all_frames":true}]`
  - `action {"default_popup":"popup.html","default_title":"ODM"}`
  - `icons {"32":"icons/32.png","128":"icons/128.png"}`
  - `minimum_chrome_version "116"`
  - **no `key`**: the build injects it.
- **Firefox:** the same, except:
  - `background {"scripts":["background.js"]}`
  - `browser_specific_settings {"gecko":{"id":"odm@odm.local","strict_min_version":"128.0"}}`

**build.mjs:**
- Remove `dist/`.
- For each target in `["chromium","firefox"]` and each entry in `["background","content","popup"]`, call `vite.build({configFile:false, logLevel:"warn", define:{__TARGET__: JSON.stringify(target)}, build:{outDir:`dist/${target}`, emptyOutDir:false, minify:false, lib:{entry:`src/${entry}.ts`, formats:["iife"], name:`odm_${entry}`, fileName:()=>`${entry}.js`}}})`. Every output is a single self-contained IIFE.
- Copy `src/popup.html` and `src/popup.css` into each target.
- Copy `../app/src-tauri/icons/32x32.png` → `icons/32.png` and `128x128.png` → `icons/128.png`.
- Write `manifest.json` from the target manifest. For Chromium, add `"key": <chromium-key.txt trimmed>`.
- This task creates placeholder `src/background.ts`, `src/content.ts`, `src/popup.ts`, `src/popup.html` and `src/popup.css` (empty modules or a minimal page) so the build runs. Task 11 fills them in.
- Declare `declare const __TARGET__: "chromium" | "firefox";` in `src/env.d.ts`.

- [ ] **Step 1: Write the failing tests** (vitest):
  - `capture.test.ts`:
    - `isHttp("HTTPS://x")` → true; `blob:`, `data:`, `ftp:` and `file:` → false.
    - `basename("C:\\Users\\a\\Downloads\\movie.mp4")` → `"movie.mp4"`; `basename("/home/a/x.zip")` → `"x.zip"`; `basename("")` → undefined.
    - `cookieHeader([{name:"a",value:"1"},{name:"b",value:"2"}])` → `"a=1; b=2"`; `cookieHeader([])` → undefined.
    - `captureUrl` prefers an http `finalUrl` over `url`, and falls back to `url` when `finalUrl` is `blob:`.
    - `shouldOffer` is false for a blob URL and false when Alt is active.
    - `toCapturePayload` with `now = 1000`: `expires_at_ms === 5500`, `source === "capture"`, `filesize` undefined for `totalBytes` −1 and 0, `headers["User-Agent"]` set.
    - `toMenuPayload` has `source "menu"` and no `expires_at_ms`.
    - `only_ok_true_cancels_the_browser_download` (Review Focus 1): `shouldCancelBrowserDownload` is true only for `{ok:true}`; it is false for `null`, `{ok:false}` and `{}`.
  - `alt.test.ts`:
    - Not active initially.
    - `down(0)` → active at 10.
    - `up(100)` → active at 1599, inactive at 1600.
  - `timeout.test.ts`: resolves the value; a never-resolving promise with 20 ms gives null; a rejected promise gives null.
  - `format.test.ts`: `formatSpeed(0)` → `"0 B/s"`, `512` → `"512 B/s"`, `1536` → `"1.5 KB/s"`, `3565158` → `"3.4 MB/s"`; `statusLine({active:2,queued:1,total_speed_bps:3565158})` → `"2 active · 1 queued · 3.4 MB/s"`.
  - `protocol.test.ts`:
    - `makeRequest("PING")` has a uuid-shaped id and `payload {}`.
    - `errorText({ok:false,error:{code:"x",message:"m"}})` → `"m"`.
    - `errorText(null)` → `"ODM did not answer."`
  - `chromium-id.test.ts`: import `idFromKey` from `../../scripts/chromium-id.mjs`, then check that `idFromKey(readFileSync("chromium-key.txt"))` equals `chromium-id.txt` trimmed, and that `HOST_NAME === "com.odm.host"`.
- [ ] **Step 2: Run** `cd extension && pnpm install && pnpm test`. Expected: failures.
- [ ] **Step 3: Implement** the modules, manifests, placeholders and `build.mjs`.
- [ ] **Step 4: Run** `pnpm check`, `pnpm test` and `pnpm build`.
  - Expected: all pass.
  - `dist/chromium` contains `manifest.json` with `key`, `background.js`, `content.js`, `popup.js`, `popup.html`, `popup.css` and `icons/`.
  - `dist/firefox` contains the same files without `key`.
  - No output file contains an `import ` statement: `grep -l "^import " dist/*/*.js` prints nothing.
- [ ] **Step 5: Commit**, including `extension/pnpm-lock.yaml`.
  - Message: `feat(extension): protocol and capture logic, manifests, two-target build`

---

### Task 11: Extension runtime: capture, Alt bypass, context menu, popup

**Files:**
- Modify: `extension/src/background.ts`, `extension/src/content.ts`, `extension/src/popup.ts`, `extension/src/popup.html`, `extension/src/popup.css`
- Create: `extension/src/lib/browser.ts` (the only module touching the browser API object)

**Interfaces:**
- Consumes: everything from Task 10.
- Produces (`browser.ts`):
  ```ts
  export const ext: typeof chrome = (globalThis as any).browser ?? (globalThis as any).chrome;
  export function send<T>(cmd: string, payload?: object, timeoutMs?: number): Promise<OdmResponse<T> | null>;
  //  = withTimeout(ext.runtime.sendNativeMessage(HOST_NAME, makeRequest(cmd, payload)), timeoutMs ?? CAPTURE_TIMEOUT_MS)
  export async function cookiesFor(url: string, storeId?: string): Promise<Cookie[]>;
  //  cookies.getAll({url, storeId}); on a Firefox error mentioning firstPartyDomain retry with {url, storeId, firstPartyDomain: null}; on any other error []
  ```

**background.ts:**
- **Alt:** `runtime.onMessage` handles `{type:"odm-alt", down: boolean}` by updating one module-level `AltTracker`.
- **Capture on Chromium** (`__TARGET__ === "chromium"`):
  - `downloads.onDeterminingFilename.addListener((item, suggest) => { handle(item).finally(() => suggest()); return true; })`.
  - `handle(item)`:
    - if `!shouldOffer(item, alt.active(Date.now()))`, return;
    - build the payload with the cookies for `captureUrl(item)` and `navigator.userAgent`;
    - `const r = await send("ADD_DOWNLOAD", payload)`;
    - if `shouldCancelBrowserDownload(r)`: `await downloads.cancel(item.id)`, then `downloads.erase({id: item.id})`, and swallow errors from both;
    - otherwise, if `r` is ok:false and its code is not `skipped`, call `flagError(errorText(r))`.
- **Capture on Firefox:** `downloads.onCreated.addListener(item => handle(item, item.cookieStoreId))`. Same flow, without `suggest`.
- **Context menu:**
  - `runtime.onInstalled` → `contextMenus.create({id: MENU_ID, title: "Download with ODM", contexts: ["link","video","audio"]})`.
  - `contextMenus.onClicked`:
    - `url = info.linkUrl ?? info.srcUrl`, and ignore it if not `isHttp`;
    - payload `toMenuPayload(url, info.pageUrl ?? tab?.url, cookies, UA)`;
    - send with a 15 000 ms timeout (the menu has no browser download to fall back to, so it may wait for a cold start);
    - on failure, call `flagError`.
- **`flagError(message)`:** sets the badge text `"!"` with background `#ef4444` for 5 s, and stores `{lastError: message, at: Date.now()}` in `storage.session` (or `storage.local` if `session` is undefined).

**content.ts:**
- On `keydown` / `keyup` with `e.key === "Alt"` (capture phase, on `window`), send `{type:"odm-alt", down}`.
- On `blur`, send `down:false`.
- Wrap the sends in try/catch: after an extension reload, the context is invalidated.

**popup:**
- On open:
  - `PING` (2000 ms timeout).
  - On ok, the status reads "Connected to ODM {version}", then `GET_SUMMARY`, shown with `statusLine`.
  - On `app_not_running`: "ODM is not running."
  - When `send` returns null or `runtime.lastError` says the host was not found: "ODM's browser helper is not installed. Open ODM once to set it up."
  - On any other error: `errorText`.
- Buttons:
  - "Pause all" / "Resume all": send the command, then refresh the summary.
  - "Open ODM": `SHOW_WINDOW` with a 15 000 ms timeout (this starts ODM if needed).
- If `lastError` is stored and less than 10 minutes old, show it as "Last problem: {message}" with a "Dismiss" button that clears it and the badge.
- **Firefox only:** if `permissions.contains({origins:["<all_urls>"]})` is false, show "ODM needs access to all sites to read download cookies." with an "Allow" button that calls `permissions.request({origins:["<all_urls>"]})`.
- Style: dark, using the app's tokens (background `#0f0f14`, surface `#14141c`, text `#e0e0e6`, muted `#6b6b80`, accent `#6366f1`), with a width of 300 px.

- [ ] **Step 1:** There is no new unit test; this task is wiring. `pnpm check` is the type gate, and Task 12 checks behaviour by hand. Before writing the code, confirm `pnpm test` still passes as a baseline.
- [ ] **Step 2: Implement** the four files and `browser.ts`.
- [ ] **Step 3: Run** `pnpm check`, `pnpm test` and `pnpm build`.
  - Expected: all pass.
  - `grep -c "onDeterminingFilename" dist/chromium/background.js` ≥ 1.
  - `grep -c "onDeterminingFilename" dist/firefox/background.js` = 0, because the `__TARGET__` branch is dead-code-eliminated. If Vite keeps the dead branch, the check is instead that the Firefox build never *registers* that listener; say which in the report.
- [ ] **Step 4: Commit**
  - Message: `feat(extension): capture, Alt bypass, context menu and popup`

---

### Task 12: Whole-flow verification and README

**Files:**
- Modify: `README.md` (a new "Browser integration" section)
- Create: `docs/superpowers/plans/2026-10-03-odm-plan-3-smoke.md` (the manual checklist with results)

- [ ] **Step 1: Automated end-to-end check (controller or implementer).**
  - Build: `cargo build -p odm-app -p odm-host`.
  - Start: `ODM_DATA_DIR=$TEMP/odm-p3-e2e ./target/debug/odm-app.exe --background`.
  - Through `odm-host.exe` with the real pipe name, send framed requests, in order:
    1. `PING` → ok;
    2. `ADD_DOWNLOAD {url:"http://127.0.0.1:8765/small.zip", source:"menu"}`, served by the scratch range server → ok with `download_id`;
    3. `GET_SUMMARY` → ok;
    4. `ADD_DOWNLOAD` with `expires_at_ms: 1` → `expired`;
    5. `{"cmd":"NOPE"}` → `unknown_command`.
  - Check that the file landed, then remove it through the UI or the `remove` command with delete.
  - Paste the replies into the smoke doc.
- [ ] **Step 2: README "Browser integration" section:**
  - build the host (`cargo build --release -p odm-host`);
  - ODM registers it on first start;
  - load `extension/dist/chromium` unpacked (`chrome://extensions` → Developer mode → Load unpacked); its ID matches `extension/chromium-id.txt`;
  - Firefox: `about:debugging` → Load Temporary Add-on (removed on restart). A permanent install needs signing as an unlisted add-on (`web-ext sign --channel=unlisted` with the user's AMO keys) or a Firefox build that allows unsigned add-ons;
  - hold Alt while clicking to let the browser download;
  - "Download with ODM" is on the right-click menu;
  - `odm-host --unregister` removes the registration.
- [ ] **Step 3: The manual smoke checklist** (written into the smoke doc; the person at the desktop ticks it):
  1. Chrome: click a large public file link (for example `https://proof.ovh.net/files/100Mb.dat`). ODM takes it, and the browser download disappears.
  2. Chrome: the same with Alt held. The browser downloads it, and ODM does nothing.
  3. Chrome: a small file (< 1 MB). The browser keeps it.
  4. Chrome: right-click a link → "Download with ODM". It appears in ODM.
  5. A cookie-gated download (any site the user is logged in to, for example a GitHub private release asset). ODM finishes it, and the file is not an HTML login page.
  6. Firefox/Floorp: items 1–4.
  7. Close ODM fully (Quit), then click a large link. ODM starts in the tray and takes the download within 5 s, or the browser keeps it. There is never a duplicate.
  8. The popup shows "Connected to ODM 0.1.0", the counts and the speed. Pause all, Resume all and Open ODM work. With ODM quit, it shows "ODM is not running."
  9. Turn capture off in ODM Settings. A large link stays in the browser.
  10. An expired link: let a captured download fail with link expired (or pause it), then click the same file in the browser again. The "Continue the earlier download?" dialog appears, and "Continue" resumes from the partial data.
- [ ] **Step 4: Run every gate** (Rust, app, extension). Expected: all pass, and the tree is clean.
- [ ] **Step 5: Commit**
  - Message: `docs: browser integration guide and Plan 3 smoke checklist`
