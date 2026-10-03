# ODM Plan 2: carry-over for Plans 3–5

Plan 2 (odm-appcore, Tauri shell, Svelte UI, Task 10b connection-limit fix, final-review fix wave) is complete on `master` at `7c9f536`.
All checks pass: fmt, clippy `-D warnings`, workspace tests (run twice), `pnpm check` 0/0, `pnpm test`, `pnpm build`.
The plan's 31 rulings are recorded in git history (commit messages) and summarised in the final report to the user. This file keeps what still needs doing.

## Do in Plan 3 (native host + browser extension touch these paths)

- `open_file` launches executables without a warning, and downloaded files get no `Zone.Identifier` (Mark of the Web). Add both before browser-captured files arrive.
- Normalise skip-hosts (lower-case, strip scheme, `www.` and port) and header names (case-insensitive merge; drop hop-by-hop headers) when the host forwards browser requests.
- `active_count` loads every row; use a `COUNT` query (the native host will call it often).
- Single instance: a second launch with `--background` (started by the native host) must not focus or show the window.
- `GET_SUMMARY`-style total speed: the host and extension popup need total speed and the active count without listing every row.
- Strip Unicode format characters (bidi overrides such as U+202E, zero-width characters) from file names taken from websites.
- Whatever the native host feeds the webview (titles, URLs, file names) is shown as text only. The strict CSP is now in place; never use `{@html}`.

## Final re-review minors (deferred)

- `download.rs:418,425`: on an early exit (persist failure, worker panic), `run_download` aborts workers without waiting for them. If the user cancelled at that moment, the part file may still be open and `delete_part` fails silently on Windows. Wait for the workers on those exits.
- `ManagerHandle::cancel` and the Tauri `cancel` command still take `delete_file`, which cancel ignores. Document this on the public method, or remove the argument when the signatures may change.
- The 408/429/5xx transient rule is written in both `odm-core/src/error.rs` (`from_code`) and `odm-engine/src/retry.rs` (`classify_status`). Share one function.

## Deferred minors from per-task reviews

- **Engine:**
  - `set_config` always calls `set_rate`, refilling the bucket and waking sleepers even when the rate is unchanged.
  - A stale `.odm-part` is left in the old folder if a re-probe re-places the download after the bad-part discard.
  - Task 10b: the connection cap never rises again within a transfer; `Retry-After` is ignored for parked chunks; parked chunks across pause/resume are untested; the safety-net branch is untested.
- **Store:** a negative `user_version` is reported as NewerSchema; the limiter wake test only covers `set_rate(0)`.
- **Appcore:**
  - Settings: the fixed `.tmp` name races concurrent saves; `safe_folder` accepts reserved device names (CON, NUL) and `...`; unknown category keys are dropped silently.
  - Logging: log rotation boundary is UTC; module-level `RUST_LOG` directives still beat the warn cap; the log guard may not flush on normal quit.
  - `redact_url` leaks a credential prefix when the password holds a raw `?`, `#` or `/`; IPv6 is untested.
  - The bridge awaits store lookups inside `select!` (a slow store can stall ticks).
  - `add_batch` drops the per-URL error reason; `suggest` probes with default headers and doesn't pre-check the scheme.
  - `save_settings` file-vs-memory ordering under concurrent calls.
  - A second instance logs "ODM starting" before handing over.
- **UI:**
  - Selection: the range-select anchor uses the last-inserted id.
  - Sorting and formatting: no natural sort for file names; `formatBytes` and `formatEta` don't handle NaN/Infinity; 1048575 B shows as "1024.0 KB"; segment `widthPct` is not clamped.
  - Store: concurrent first `init()` calls can register two listeners; a failed `listDownloads` leaves the reload gate in flight; replay can briefly regress a row.
  - Settings: native min/max blocks out-of-range values before the backend clamps them; the form doesn't show the clamped result until reopened; `min_capture_bytes` rounds to 0.1 MB on every save; `speed_limit_kbps` has no max; a failed `pickFolder` is silent.
  - Dialogs: pressing on the backdrop and releasing inside the dialog still closes it; the typing guard ignores select/contenteditable.
  - Downloads: a running row shows the requested name (`video.mp4`) until it finishes and becomes `video (1).mp4`.
  - Misc: `copyUrl` timeout never cleared; the sticky `th` border is lost with `border-collapse`.
- **Repo:** unused Android/iOS/UWP icons committed (~100 KB); TypeScript pinned to ^6 because svelte-check rejects TS 7; `odm-cli/src/lib.rs` has LF endings.

## Not verified by the controller (needs a person at the desktop)

Tray icon and menu, close-to-tray, the quit dialog, the completion notification, second-launch focus, Open file/folder, the folder picker, column hiding on window resize, the startup-failure dialog, and a re-run of the OVH CLI command that originally failed with HTTP 429.
