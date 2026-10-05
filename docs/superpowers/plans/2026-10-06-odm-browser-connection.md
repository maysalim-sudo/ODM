# Browser connection panel (and the size hint)

**Goal:** someone who just installed ODM can see, inside ODM:
- whether each browser can reach it;
- prove the whole path works with one click;
- repair the registration;
- open the bundled extension folder to load it.

Separately, the New download dialog shows the size the browser already reported.

**Constraints:**
- These still hold: Plan 3's Global Constraints, the UI copy rules (sentence case, plain words, no exclamation marks) and the dark theme tokens.
- Registry reads only, except "Repair", which runs the existing `odm-host --register --app <exe>`.
- **Gates:** Rust fmt, clippy `-D warnings` and workspace tests; app `pnpm check` 0/0, `pnpm test` and `pnpm build`.
- One commit per task with the `Co-Authored-By` line. Do not push.

## Task 1: appcore and engine (Rust)

**Interfaces:**
```rust
// odm-appcore/src/integration.rs
#[derive(Debug, Clone, PartialEq, Serialize)] #[serde(rename_all = "snake_case")]
pub enum Check { Ok, Missing, WrongPath, Broken }      // Broken: the manifest file is unreadable or not valid JSON
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BrowserCheck { pub browser: String, pub status: Check, pub detail: String }
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IntegrationStatus {
    pub host_found: bool,                 // odm-host.exe exists next to the app exe
    pub pipe_running: bool,
    pub browsers: Vec<BrowserCheck>,      // in this order: Chrome, Edge, Brave, Vivaldi, Opera, Chromium, Firefox and Floorp
    pub last_contact_ms: Option<u64>,     // epoch ms of the last IPC request from a browser
    pub last_command: Option<String>,
}
/// Pure: given how to read a registry default value and a file, judge one browser's registration.
pub fn check_browser(browser: &str, manifest_path: Option<String>, read_file: impl Fn(&str) -> Option<String>, host_exe: &Path) -> BrowserCheck;
#[cfg(windows)] pub fn status(host_exe: &Path, pipe_running: bool, contact: &Contact) -> IntegrationStatus;
/// Runs `<host_exe>` with ODM_PIPE_NAME inherited, writes one framed PING, reads one framed reply (timeout 10 s).
/// Ok(version) on ok:true; Err(plain sentence) otherwise.
pub fn self_test(host_exe: &Path) -> Result<String, String>;
/// Runs `<host_exe> --register --app <app_exe>` (CREATE_NO_WINDOW), waits up to 10 s. Err(plain sentence) on failure.
pub fn repair(host_exe: &Path, app_exe: &Path) -> Result<(), String>;
// Last contact, shared with ipc::dispatch:
#[derive(Default)] pub struct Contact { /* Mutex<Option<(u64, String)>> */ }
impl Contact { pub fn record(&self, cmd: &str); pub fn get(&self) -> Option<(u64, String)>; }
```

**Details:**
- Use the registry key lists from odm-host's `register.rs`: `CHROMIUM_KEYS` and `MOZILLA_KEY` + `\com.odm.host`. Do not depend on odm-host: copy the 7 key paths into appcore as a const, with a comment naming the source.
- Browser name for each key:
  - `Software\Google\Chrome` → "Chrome";
  - `Software\Microsoft\Edge` → "Edge";
  - `BraveSoftware` → "Brave";
  - `Vivaldi` → "Vivaldi";
  - `Opera Software` → "Opera";
  - `Chromium` → "Chromium";
  - `Mozilla` → "Firefox and Floorp".
- **`check_browser`:**
  - no registry value → `Missing` with "Not registered.";
  - the manifest file is unreadable or invalid JSON → `Broken` with "The registration file is damaged.";
  - its `path` differs (case-insensitive, `/` or `\`) from `host_exe` → `WrongPath` with "Registered for another copy of ODM.";
  - otherwise `Ok` with "Ready.".
- **`self_test` failure messages, exactly:**
  - "The browser helper (odm-host.exe) is missing next to ODM." (the file doesn't exist);
  - "The browser helper could not be started.";
  - "The browser helper did not answer.";
  - for an `ok:false` reply: "ODM did not answer the browser helper: <error.message>".
- **`IpcContext`** gains `pub contact: Arc<Contact>`. `dispatch` calls `contact.record(&req.cmd)` for every request. The Tauri side passes the same `Arc` to the status command.
- **Size hint:**
  - engine `AddRequest` gains `pub size_hint: Option<u64>`. When it is `Some` and positive, the new record's `size` is set to it. The probe overwrites it as usual.
  - appcore `ipc` `ADD_DOWNLOAD` sets `size_hint = payload.filesize`.

**Tests (write first):**
- `check_browser` for each of the 4 outcomes.
- `Contact`: record then get returns the command, and the time is within 1 s of `now_ms()`.
- `self_test`:
  - against a missing exe → the exact missing message;
  - an integration test that starts a pipe server (reuse `tests/common`), sets `ODM_PIPE_NAME` for the child, and uses the real built `odm-host` (find it next to the test exe in `target/<profile>/odm-host.exe` after `cargo build -p odm-host`; skip with `eprintln!` if absent) → `Ok(version)`.
- ipc: a dispatched PING updates `Contact`.
- engine `size_hint_sets_initial_size`: add paused with `size_hint: Some(123_456)` → the record's size is 123 456.
- ipc `add_download_passes_filesize_hint`.

**Commit:** `feat: browser connection status, self-test and repair; size hint from the browser`

## Task 2: Tauri commands, bundled extension, UI

**Tauri:**
- `AppState` gains `contact: Arc<Contact>` and `pipe_running: AtomicBool`, set from the `serve` result.
- New commands:
  - `integration_status() -> IntegrationStatus`;
  - `integration_test() -> Result<String, String>`, run on a blocking thread;
  - `integration_repair() -> Result<IntegrationStatus, String>`;
  - `open_extension_folder(kind: String)`, where `kind` is `"chromium"` or `"firefox"`. It opens `<resource_dir>/extension/<kind>` with the opener plugin, and returns `Err("The extension files are not included in this copy of ODM.")` if missing.
- **Bundle the extension:** in `tauri.conf.json`, `bundle.resources` is a map:
  ```json
  {"../../extension/dist/chromium/": "extension/chromium/", "../../extension/dist/firefox/": "extension/firefox/"}
  ```
  `scripts/release.mjs` already builds the extension before the app, so leave that order as it is. For `pnpm tauri build` without a built extension, the build must still succeed: check what Tauri does with a missing resource. If it fails, make the release script's order the documented requirement in the README and say so in the report.

**UI:**
- `api.ts` wrappers for the four commands. `types.ts` gets `IntegrationStatus` and `BrowserCheck`.
- **Sidebar footer chip** (bottom of the sidebar):
  - "Browser: connected" + relative time ("just now", "3 min ago", "2 h ago") when `last_contact_ms` is set;
  - "Browser: not connected yet" otherwise;
  - an amber dot if any browser check isn't Ok, or `pipe_running` is false, or `host_found` is false.
  
  It refreshes every 30 s and after the dialog closes. Clicking it opens BrowserDialog.
- **New `BrowserDialog.svelte`** (on `Modal`, width 560):
  - **Title:** "Browser connection".
  - **Summary line:**
    - "Everything is ready." when all is OK;
    - else "Something needs attention." plus the first problem in plain words, e.g. "The browser helper is missing next to ODM." or "ODM couldn't open its connection for browsers. Another copy of ODM may be running."
  - **List:** one row per browser: name, a status pill (Ready / Not registered / Other copy / Damaged), and the detail text.
  - **Last contact:** "Last message from a browser: 3 min ago (ADD_DOWNLOAD)" or "No browser has contacted ODM since it started."
  - **Buttons:**
    - "Test connection": shows "Working. ODM <version> answered through the browser helper." or the error, inline;
    - "Repair": runs repair, refreshes the list, and shows "Registration repaired." or the error.
  - **"Add the extension" block, with two short instruction lists:**
    - Chromium browsers: Open `chrome://extensions` (or your browser's extensions page), turn on Developer mode, choose Load unpacked, pick the folder. The block has an "Open Chromium extension folder" button.
    - Firefox and Floorp: Open `about:debugging`, This Firefox, Load Temporary Add-on, pick `manifest.json` in the folder. The block has an "Open Firefox extension folder" button.
  - **Close.**
- **`model.ts`:** `export function relativeTime(ms: number, now: number): string` (tested first): `<45 s` "just now"; `<90 min` "N min ago"; `<36 h` "N h ago"; else "N days ago". Also `export function integrationProblem(s: IntegrationStatus): string | null`, tested first with each case.
- The Delete-key guard covers BrowserDialog like the other dialogs.

**Verification:**
- Run the gates.
- Build `pnpm tauri build --debug --no-bundle --config '{"identifier":"com.odm.conntest"}'` (a different identifier, so you don't touch the user's running ODM).
- Launch it from PowerShell with a throwaway `ODM_DATA_DIR`, a throwaway `ODM_PIPE_NAME` and WebView2 port 9333.
- Open the dialog through CDP: Test connection should succeed (it runs the debug `odm-host.exe` with the inherited pipe name). Take a screenshot if you can.
- Kill only the processes you started.

**Commit:** `feat(ui): Browser connection panel with test, repair and the bundled extension`
