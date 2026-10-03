# ODM Plan 5: Polish and Installer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A one-command release build that produces an unsigned NSIS installer for personal use. The installer ships `ODM.exe` and `odm-host.exe` side by side, registers the browser helper on first start, and unregisters it on uninstall. The plan also closes the highest-value carry-over items.

**Architecture:**
- Tauri's `externalBin` bundles `odm-host` next to the main binary.
- An NSIS installer hook runs `odm-host --unregister` before uninstalling.
- `scripts/release.mjs` builds everything in order.
- The extension popup learns to tell a missing helper apart from a slow one.

**Tech Stack:** the existing ones. Tauri 2 bundler (NSIS), Node 24.

**Spec:** `docs/superpowers/specs/2026-09-29-odm-design.md`, section 12 phase 5 ("a Tauri installer bundle for personal use (MSI/NSIS, unsigned), first-run registration, and README") and section 8 (registration).
**Carry-over:** `docs/superpowers/plans/2026-10-03-odm-plan-2-carryover.md`, the "Final re-review minors".

## Global Constraints

- Plan 3's Global Constraints still apply.
- The installer is unsigned and per-user (`installMode: "currentUser"`), so it needs no admin rights. The main binary is named `ODM.exe` (`mainBinaryName: "ODM"`).
- Never commit build outputs: `app/src-tauri/binaries/` is git-ignored.
- Release builds use `--release`. Debug flows in the README stay valid.

## Review Focus

1. **Uninstall leaves no registration behind.** The HKCU `com.odm.host` keys and `Software\ODM` are gone afterwards. *Check: Task 2 Step 4.*
2. **Moving or reinstalling to another folder** re-registers the helper at its new path on the next start. This was already done in Plan 3 Task 8 (`registration_needed`); re-verify it with the installed build. *Check: Task 2 Step 4.*
3. **The popup when the helper is missing, versus when ODM is slow to answer:** the two show different messages. *Test: Task 1 `classifies_send_failures`.*

---

### Task 1: Carry-over fixes and the popup's helper-missing message

**Files:**
- Modify: `crates/odm-engine/src/download.rs` (early exits wait for workers), `crates/odm-engine/src/retry.rs`, `crates/odm-core/src/error.rs` (one shared transient-status rule), `crates/odm-engine/src/manager.rs` (doc on `cancel`, if it is still missing), `crates/odm-appcore/src/settings.rs` (unique temp file name in `save`), `extension/src/lib/browser.ts`, `extension/src/lib/protocol.ts`, `extension/src/popup.ts`
- Test: `crates/odm-core/src/error.rs` (unit), `crates/odm-appcore/tests/settings.rs`, `extension/src/lib/protocol.test.ts`

**Interfaces:**
- `odm_core::ErrorKind::from_http_status(status: u16, started: bool) -> ErrorKind`:
  - 408, 429 or 5xx → `HttpTransient(s)`;
  - 403 or 410 with `started` → `LinkExpired`;
  - otherwise `Http(s)`.
  - `retry::classify_status` becomes a call to it, and `from_code` uses the same transient set through a shared private `fn is_transient_status(u16) -> bool`.
- `download.rs`: on the early-exit paths of `transfer` (a persist failure, a worker panic), abort the remaining workers **and await them** (drain the `JoinSet`) before returning, so that no write still holds the part file.
- `Settings::save`: the temp file is `settings.json.<pid>-<counter>.tmp`, using a process-wide `AtomicU64`, so concurrent saves never share a temp file.
- Extension:
  ```ts
  // protocol.ts
  export type SendFailure = "missing_host" | "timeout" | "other";
  export function classifySendError(e: unknown): SendFailure;   // message contains "native messaging host not found" or "Specified native messaging host" (case-insensitive) → missing_host; "timeout" sentinel → timeout; else other
  // browser.ts
  export function sendDetailed<T>(cmd: string, payload?: object, timeoutMs?: number): Promise<{ response: OdmResponse<T> | null; failure?: SendFailure }>;
  // send() stays as is (returns null on any failure) and is implemented on top of sendDetailed.
  ```
- Popup messages:
  - `missing_host` → "ODM's browser helper is not installed. Open ODM once to set it up."
  - `timeout` → "ODM did not answer in time."
  - `other` → "Could not reach ODM."

- [ ] **Step 1: Write the failing tests:**
  - `from_http_status_cases`:
    - 503 / any → transient;
    - 429 → transient;
    - 404 → `Http(404)`;
    - 403 started → `LinkExpired`; 403 not started → `Http(403)`;
    - 410 started → `LinkExpired`.
  - `concurrent_saves_do_not_clash`: 8 threads each save different settings to the same path at once. Every save returns `Ok`. The final file parses and equals one of the 8. No `.tmp` files are left in the directory.
  - `classifies_send_failures`:
    - `new Error("Specified native messaging host not found.")` → `missing_host`;
    - `new Error("Error when communicating with the native messaging host.")` → `other`;
    - the timeout sentinel → `timeout`.
- [ ] **Step 2: Run** the tests. Expected: failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** every gate (Rust, app, extension). Expected: pass.
- [ ] **Step 5: Commit**
  - Message: `fix: shared HTTP status rule, safe settings saves, wait for workers on early exit, clearer popup errors`

---

### Task 2: Installer bundle, release script, uninstall hook

**Files:**
- Modify: `app/src-tauri/tauri.conf.json`, `.gitignore`, `README.md`
- Create: `app/src-tauri/installer-hooks.nsh`, `scripts/release.mjs`

**Config (`tauri.conf.json`):**
- Top level: `"mainBinaryName": "ODM"`.
- `bundle.externalBin: ["binaries/odm-host"]`. Tauri expects `binaries/odm-host-x86_64-pc-windows-msvc.exe` at build time and installs it as `odm-host.exe` next to `ODM.exe`.
- `bundle.windows.nsis`: `{ "installMode": "currentUser", "installerHooks": "./installer-hooks.nsh" }`.
- Keep `targets: ["nsis"]`.

**`installer-hooks.nsh`:**
```nsis
!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\odm-host.exe" --unregister'
!macroend
```

**`scripts/release.mjs`** (run from the repo root with `node scripts/release.mjs`). It stops at the first failing step, printing which one:
1. `cargo build --release -p odm-host`.
2. Copy `target/release/odm-host.exe` to `app/src-tauri/binaries/odm-host-<triple>.exe`. Get the triple from `rustc -vV` (its `host:` line).
3. `pnpm install --frozen-lockfile` and `pnpm build` in `extension/`.
4. `pnpm install --frozen-lockfile` and `pnpm tauri build` in `app/`.
5. Print the installer path (`target/release/bundle/nsis/*.exe`) and the extension folders.

**`.gitignore`:** add `app/src-tauri/binaries/`.

**README:** add an "Install" section:
- `node scripts/release.mjs`, then run the installer from `target\release\bundle\nsis\`;
- ODM registers the browser helper on its first start;
- uninstalling removes the registration;
- load the extension from `extension\dist\…` as described under "Browser integration".
Also update "Status" to say everything in v1 is built.

- [ ] **Step 1:** Make the config and file changes.
- [ ] **Step 2: Run** `node scripts/release.mjs`. Expected: it finishes, and `target/release/bundle/nsis/ODM_0.1.0_x64-setup.exe` exists. If NSIS tooling must be downloaded by the Tauri CLI, that download is expected.
- [ ] **Step 3: Check the bundle without installing:**
  - `target/release/ODM.exe` exists, and `app/src-tauri/binaries/odm-host-*.exe` was used.
  - Optionally list the archive with `7z l` if 7-Zip is present. Otherwise skip, and say so.
- [ ] **Step 4: Installed-build check** (the controller does it, not the implementer):
  - Silent install into a temp folder: `ODM_0.1.0_x64-setup.exe /S /D=<temp>\ODM-install`.
  - Start `<temp>\ODM-install\ODM.exe --background` with a throwaway `ODM_DATA_DIR`.
  - `HKCU\Software\ODM\HostPath` points at `<temp>\ODM-install\odm-host.exe`.
  - Quit, then run the uninstaller silently (`uninstall.exe /S`). `HKCU\Software\ODM` and `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.odm.host` are gone.
  - Re-run the dev registration afterwards by starting the debug app once.
- [ ] **Step 5:** Run every gate. Commit with the message `build: NSIS installer with the browser helper, release script, uninstall hook`.
