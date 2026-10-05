# "New download" dialog for browser downloads

**Goal:** when the browser hands ODM a download, ODM comes to the front and asks where to save it and whether to start it now or later, like IDM's "Download File Info" dialog.

**Flow:**
1. A browser download arrives (`ADD_DOWNLOAD` from a capture, the right-click menu or the video button, or `ADD_STREAM`) and the setting `confirm_browser_downloads` is on (the default).
2. The app adds the download **paused** (`start_paused = true`) so it is persisted at once, replies `ok:true` (the browser cancels its own copy), records the id as "awaiting confirmation", emits `UiEvent::ConfirmDownload { id }`, and shows the window.
3. The UI shows the "New download" dialog for that id. Several queue up and are shown one after another (oldest first).
4. The user's choice:
   - **Start now:** apply the name and folder, then `resume` (it starts when a slot is free).
   - **Later:** apply the name and folder, and leave it paused.
   - **Cancel:** remove the record (part file deleted, as with remove).
   
   Closing the dialog or pressing Escape counts as **Later**, so nothing is lost.
5. The dialog has a "Don't ask again" checkbox. It saves `confirm_browser_downloads = false`.

When the setting is off, behaviour is exactly as today: the download is added and started.

The reuse offer still comes first. If a reuse offer is created (Plan 3), the confirm dialog is not used for that request.

**Constraints:**
- These still hold: Plan 3's Global Constraints, UI copy rules, and the dark theme tokens.
- Gates: Rust fmt, clippy `-D warnings` and workspace tests; app `pnpm check` (0/0), `pnpm test` and `pnpm build`.
- One commit per task, each ending with the `Co-Authored-By` line. Do not push.

## Task 1: engine and appcore

**Interfaces:**
```rust
// odm-engine ManagerHandle
/// Change name and/or folder of a download that has not transferred any byte yet (state Paused or Queued,
/// no chunk with written > 0, not active). The name is sanitized and becomes locked (the probe won't rename it);
/// a folder turns off auto-placement. Errors otherwise: "this download has already started".
pub async fn set_target(&self, id: &str, filename: Option<String>, save_dir: Option<PathBuf>) -> Result<(), OdmError>;
// odm-store: a small `set_target(id, filename: Option<String>, save_dir: Option<String>)` repo fn (one UPDATE;
//   also sets filename_locked=1 when a name is given and auto_place=0 when a folder is given).
// odm-appcore settings: `pub confirm_browser_downloads: bool` (default true; tolerant parsing like the others).
// odm-appcore confirm.rs
#[derive(Default)] pub struct Pending { /* Mutex<Vec<String>> oldest first, no duplicates */ }
impl Pending { pub fn push(&self, id: String); pub fn list(&self) -> Vec<String>; pub fn remove(&self, id: &str) -> bool; }
// IpcContext gains: pub pending: Arc<Pending>, pub on_confirm: Box<dyn Fn(&str) + Send + Sync>
// dto.rs: UiEvent::ConfirmDownload { id: String }  -> {"type":"confirm_download","id":"..."}
// api.rs
#[derive(Deserialize)] pub struct ConfirmArgs { pub id: String, pub filename: Option<String>, pub save_dir: Option<String>, pub action: String /* "start" | "later" | "cancel" */ }
pub async fn confirm_download(m: &ManagerHandle, pending: &Pending, a: ConfirmArgs) -> Result<(), String>;
```
- **`ipc.rs`:** for `ADD_DOWNLOAD` (after the existing expired, skipped and reuse-offer steps) and `ADD_STREAM`, when the settings say `confirm_browser_downloads`:
  - set `req.start_paused = true` (and the equivalent on `StreamRequest`), then add;
  - `pending.push(id)`, `on_confirm(&id)`, `show_window()`;
  - reply `Added{download_id}`.
- **`confirm_download`:**
  1. Remove the id from `pending` (an unknown id is fine).
  2. For `start` / `later`, call `set_target` when a name or folder is given (blank strings count as `None`). If `set_target` fails because the download has already started, ignore that and continue.
  3. Then:
     - `start` → `resume`;
     - `later` → nothing more;
     - `cancel` → `remove(id, false)`;
     - any other action → `Err("Unknown action.")`.
- **Tests (write them first):**
  - store `set_target_updates_and_locks`.
  - engine `set_target_renames_and_moves_before_start`: an added paused download gets `set_target(Some("new name.zip"), Some(tmp2))`, then `resume`. It completes at `tmp2/new name.zip`.
  - engine `set_target_refuses_after_bytes_were_written`.
  - appcore ipc `confirm_setting_adds_paused_and_asks`:
    - with the setting on: the reply is ok, the record is Paused, `pending.list() == [id]`, and the `on_confirm` and `show_window` counters are 1;
    - with it off: the record is not paused, and `pending` is empty.
  - appcore api `confirm_actions`:
    - `start` with a new name → the download completes under that name;
    - `later` → stays Paused and is no longer pending;
    - `cancel` → the record is gone;
    - `"nope"` → Err.
  - settings `confirm_browser_downloads_defaults_true_and_parses`.
- **Commit:** `feat: ask where to save browser downloads and whether to start now`

## Task 2: Tauri commands and the dialog

- **Tauri:**
  - `AppState` gains `pending: Arc<Pending>`. The `IpcContext.on_confirm` closure emits `UiEvent::ConfirmDownload` on `odm://event`.
  - New commands: `list_pending_confirms() -> Vec<String>` and `confirm_download(args: ConfirmArgs) -> Result<(), String>`.
- **UI:**
  - `types.ts`: `UiEvent` gains `{type: "confirm_download"; id: string}`.
  - `api.ts`: `listPendingConfirms()` and `confirmDownload(args)`.
  - **Store** (`downloads.svelte.ts`): `confirms: string[]`. It is loaded on `init()` and appended (no duplicates) on the event. `applyEvent` ignores `confirm_download`. Add a model test in the same style as `reuse_offer`.
  - **`model.ts`:** `export function nextConfirm(ids: string[], rows: Row[]): Row | undefined` returns the first id that has a row in state `paused`. Pending ids whose row is gone or no longer paused are skipped; the store drops them. Write the test first.
- **New `ConfirmDialog.svelte`** (on `Modal`, width 520):
  - **Title:** "New download".
  - **Read-only lines:**
    - the host of the URL;
    - the size if known: "Size: 12.3 MB", or "Size: unknown".
  - **"File name"** input, prefilled with `row.filename`.
  - **"Save to"** input, prefilled with `row.save_dir`, plus a **"Browse…"** button (the existing `pickFolder`).
  - **Checkbox:** "Don't ask again (you can turn this back on in Settings)".
  - **Buttons:** "Cancel" (secondary, left), "Later" (secondary), "Start now" (primary).
    - Escape or closing counts as Later.
    - A `busy` guard prevents double clicks.
    - Errors are shown inline.
  - **On any choice:**
    - call `confirmDownload`;
    - if the checkbox is ticked, load the settings, set `confirm_browser_downloads = false` and save them;
    - drop the id from `confirms`, so the next dialog (if any) appears.
- **Precedence:** the ReuseDialog keeps priority when both are pending.
- **SettingsDialog:** checkbox "Ask before downloading from the browser" bound to `confirm_browser_downloads`, placed next to the capture settings.
- **Verification:**
  - Run the gates.
  - Build the app (`pnpm tauri build --debug --no-bundle`).
  - Launch it with a throwaway `ODM_DATA_DIR` and a throwaway `ODM_PIPE_NAME`, from PowerShell.
  - Send a framed `ADD_DOWNLOAD` (`source: "menu"`) through `target/debug/odm-host.exe`. The dialog appears; check it with the WebView2 debug port (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`) if you can, and say plainly if you couldn't.
  - Kill the app afterwards.
- **Commit:** `feat(ui): New download dialog with Start now, Later and Cancel`
