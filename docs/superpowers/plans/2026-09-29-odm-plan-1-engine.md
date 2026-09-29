# ODM Plan 1: Workspace, Core, Store, Engine, CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a fully tested Rust download engine that you can drive from the command line (`odm-cli get <url>`). It supports multiple connections, dynamic splitting, resume, retries, a queue and a speed limit.

**Architecture:**
- A Cargo workspace of small crates:
  - `odm-core`: pure types and helpers.
  - `odm-store`: SQLite owned by one dedicated thread, called asynchronously.
  - `odm-engine`: tokio + reqwest. One task per download and a manager actor.
  - `odm-testserver`: an axum server with fault injection, used only by tests.
  - `odm-cli`: a development binary.
- Later plans (Tauri UI, native host, extension, HLS) build on this plan's public API.

**Tech Stack:** Rust stable (1.98, MSVC), tokio 1, reqwest 0.12 (rustls, stream), rusqlite 0.32 (bundled), tokio-util 0.7 (CancellationToken), thiserror 2, serde/serde_json 1, tracing 0.1, fastrand 2, axum 0.8 (tests), sha2 0.10 (tests), clap 4 (CLI), uuid 1.

**Spec:** `docs/superpowers/specs/2026-09-29-odm-design.md` (sections 2, 3, 4.1, 5, 7, 10, 11). Read it before starting.

## Global Constraints

- The product name is `ODM`; the host name is `com.odm.host`; the Firefox ID is `odm@odm.local`; the app data directory is `%APPDATA%\ODM\`; the pipe name is `\\.\pipe\odm-<windows-username>`. All of these live **only** in `odm-core::ids`.
- The crate dependency direction is `odm-core` ← `odm-store` ← `odm-engine` ← (`odm-cli`, the app later). `odm-core` does no I/O.
- Download states are exactly: `queued, probing, downloading, paused, merging, completed, failed, cancelled`. There is no `error` state.
- `max_connections`: default 8, range 1–32. `max_concurrent`: default 3, range 1–10. Retry attempts per chunk: default 5. Backoff is 1, 2, 4 … s with a 60 s cap and ±20% jitter, and it honours `Retry-After`.
- Minimum chunk size is 1 MiB. Dynamic splitting only happens when the remaining bytes exceed 2 MiB.
- Progress is persisted at most once per second per download, and always on pause, stop or finish. It is always persisted **after** the file data has been `sync_data()`'d.
- Speed is a 5 s moving average of bytes received in this session. Resumed bytes are excluded.
- Cookie and Authorization header values are never logged. URLs are logged without the query string.
- Filenames are sanitized as described in spec §5.3. Name collisions produce `name (1).ext`.
- Shutdown cancels everything and waits at most 3 s.
- **One deliberate refinement of the spec:** the part file is named `<final-name>.<first 8 chars of id>.odm-part`, not `<final>.odm-part`. This stops two downloads with the same name from sharing a part file.
- Every shell command in this plan is run from the repo root `E:\Data From Desktop\Coding\ODM` in Git Bash, with `~/.cargo/bin` on PATH.

## Review Focus

These are inputs the spec implies but doesn't spell out. The task that owns each case must test it:
1. **A 0-byte file.** The server answers the probe with `416`, or `200` with `Content-Length: 0`. The download must complete as an empty file without calling `set_len` on a missing size or hanging. *Test: Task 9 (`probe_zero_length_416`) and Task 12 (`zero_byte_download_completes`).*
2. **A `206` whose `Content-Range` start differs from the requested offset.** This is a broken proxy or CDN. It must be treated as `RangeIgnored`, never written at the wrong offset. *Test: Task 10 (`wrong_content_range_start_is_range_ignored`).*
3. **The save directory is missing, or the path is actually a file.** A missing directory is created. A path that points at a file fails cleanly with a `failed` state and an I/O or permission error, with no panic. *Test: Task 12 (`save_dir_is_a_file_fails_cleanly`).*
4. **A long or Unicode `filename*=UTF-8''…` in Content-Disposition.** It is decoded, sanitized and truncated to 200 characters, and the extension is kept. *Test: Task 3 (`long_utf8_disposition_truncated_keeps_ext`).*
5. **The same URL added twice while the first is still active.** Both downloads complete as separate files (`x.bin` and `x (1).bin`) and their part files never collide. *Test: Task 13 (`same_url_twice_both_complete`).*

---

## File Structure

```
ODM/
  Cargo.toml                          workspace + shared dependency versions
  .gitignore
  rust-toolchain.toml                 pins stable
  crates/
    odm-core/
      Cargo.toml
      src/lib.rs                      re-exports
      src/ids.rs                      product identity constants + pipe_name()
      src/state.rs                    DownloadState + transitions + str conversion
      src/error.rs                    ErrorKind (transient/permanent) + OdmError
      src/filename.rs                 sanitize, unique_path, from_url, from_content_disposition
      src/category.rs                 Category + detect()
    odm-store/
      Cargo.toml
      src/lib.rs                      Store (owner thread) + StoreError
      src/migrations.rs               embedded ordered migrations, user_version
      src/models.rs                   DownloadRecord, ChunkRecord, ProbeMeta
      src/repo.rs                     SQL functions taking &Connection
      tests/store.rs
    odm-testserver/
      Cargo.toml
      src/lib.rs                      TestServer with fault-injection config
    odm-engine/
      Cargo.toml
      src/lib.rs                      re-exports public API
      src/config.rs                   EngineConfig
      src/limiter.rs                  SpeedLimiter (token bucket with debt)
      src/retry.rs                    backoff_delay, classify_status/io/reqwest
      src/chunks.rs                   ChunkShared, ChunkTable (plan, claim, split, snapshot)
      src/probe.rs                    probe() + ProbeResult
      src/worker.rs                   run_chunk() + ChunkCtx
      src/speed.rs                    SpeedMeter (5 s moving average)
      src/events.rs                   EngineEvent, ProgressSnapshot
      src/download.rs                 run_download() + Outcome
      src/manager.rs                  Manager actor + ManagerHandle + AddRequest
      tests/limiter.rs, tests/probe.rs, tests/worker.rs,
      tests/download.rs, tests/manager.rs, tests/regression.rs
    odm-cli/
      Cargo.toml
      src/main.rs
```

---

### Task 0: Workspace skeleton

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `rust-toolchain.toml`
- Create: `crates/odm-core/Cargo.toml`, `crates/odm-core/src/lib.rs`

**Interfaces:**
- Produces: a workspace that later tasks add crates to, and the `[workspace.dependencies]` versions they reference with `dep.workspace = true`.

- [ ] **Step 1: Write the workspace files**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
edition = "2021"
version = "0.1.0"
license = "MIT"

[workspace.dependencies]
odm-core = { path = "crates/odm-core" }
odm-store = { path = "crates/odm-store" }
odm-engine = { path = "crates/odm-engine" }
odm-testserver = { path = "crates/odm-testserver" }
tokio = { version = "1", features = ["full"] }
tokio-util = "0.7"
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "stream", "gzip"] }
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
fastrand = "2"
uuid = { version = "1", features = ["v4"] }
futures-util = "0.3"
axum = "0.8"
bytes = "1"
sha2 = "0.10"
tempfile = "3"
clap = { version = "4", features = ["derive"] }
percent-encoding = "2"
```

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "stable"
```

`.gitignore`:
```
/target
**/*.odm-part
node_modules/
dist/
```

`crates/odm-core/Cargo.toml`:
```toml
[package]
name = "odm-core"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
thiserror.workspace = true
percent-encoding.workspace = true
```

`crates/odm-core/src/lib.rs`:
```rust
//! Shared pure types and helpers for ODM. No I/O lives here.
```

- [ ] **Step 2: Verify the workspace builds**

Run: `cargo build`
Expected: `Finished` with no errors.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml rust-toolchain.toml .gitignore crates/odm-core
git commit -m "chore: cargo workspace skeleton"
```

---

### Task 1: `odm-core`: identity constants and download states

**Files:**
- Create: `crates/odm-core/src/ids.rs`, `crates/odm-core/src/state.rs`
- Modify: `crates/odm-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `odm_core::ids::{PRODUCT_NAME, BUNDLE_ID, HOST_NAME, FIREFOX_EXT_ID, APP_DIR_NAME, pipe_name(user: &str) -> String}`
  - `odm_core::state::DownloadState` (Copy, Eq, serde lowercase) with:
    - `as_str(&self) -> &'static str`
    - `parse(s: &str) -> Option<DownloadState>`
    - `can_transition(self, to: DownloadState) -> bool`
    - `is_terminal(self) -> bool`
    - `is_active(self) -> bool` (probing, downloading or merging)

- [ ] **Step 1: Write the failing tests (inline `#[cfg(test)]` in `state.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::DownloadState::*;
    use super::*;

    #[test]
    fn round_trips_strings() {
        for s in [Queued, Probing, Downloading, Paused, Merging, Completed, Failed, Cancelled] {
            assert_eq!(DownloadState::parse(s.as_str()), Some(s));
        }
        assert_eq!(DownloadState::parse("error"), None);
    }

    #[test]
    fn allowed_transitions() {
        assert!(Queued.can_transition(Probing));
        assert!(Probing.can_transition(Downloading));
        assert!(Downloading.can_transition(Paused));
        assert!(Paused.can_transition(Queued));
        assert!(Downloading.can_transition(Completed));
        assert!(Downloading.can_transition(Merging));
        assert!(Merging.can_transition(Completed));
        assert!(Failed.can_transition(Queued)); // manual retry
        assert!(Downloading.can_transition(Queued)); // reset after crash
    }

    #[test]
    fn forbidden_transitions() {
        assert!(!Completed.can_transition(Downloading));
        assert!(!Cancelled.can_transition(Queued));
        assert!(!Queued.can_transition(Completed));
        assert!(!Paused.can_transition(Downloading)); // must go through Queued
    }

    #[test]
    fn terminal_and_active() {
        assert!(Completed.is_terminal() && Cancelled.is_terminal() && Failed.is_terminal());
        assert!(!Paused.is_terminal());
        assert!(Probing.is_active() && Downloading.is_active() && Merging.is_active());
        assert!(!Queued.is_active());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-core`
Expected: compile error, "cannot find type `DownloadState`".

- [ ] **Step 3: Implement**

`crates/odm-core/src/ids.rs`:
```rust
//! Product identity. The ONLY place these values are defined.
pub const PRODUCT_NAME: &str = "ODM";
pub const BUNDLE_ID: &str = "com.odm.app";
pub const HOST_NAME: &str = "com.odm.host";
pub const FIREFOX_EXT_ID: &str = "odm@odm.local";
pub const APP_DIR_NAME: &str = "ODM";

/// Named pipe path for the given Windows user name.
pub fn pipe_name(user: &str) -> String {
    format!(r"\\.\pipe\odm-{user}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn pipe_name_format() {
        assert_eq!(super::pipe_name("Torinou"), r"\\.\pipe\odm-Torinou");
    }
}
```

`crates/odm-core/src/state.rs` (above the tests module):
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadState {
    Queued,
    Probing,
    Downloading,
    Paused,
    Merging,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadState {
    pub fn as_str(&self) -> &'static str {
        use DownloadState::*;
        match self {
            Queued => "queued",
            Probing => "probing",
            Downloading => "downloading",
            Paused => "paused",
            Merging => "merging",
            Completed => "completed",
            Failed => "failed",
            Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        use DownloadState::*;
        Some(match s {
            "queued" => Queued,
            "probing" => Probing,
            "downloading" => Downloading,
            "paused" => Paused,
            "merging" => Merging,
            "completed" => Completed,
            "failed" => Failed,
            "cancelled" => Cancelled,
            _ => return None,
        })
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Probing | Self::Downloading | Self::Merging)
    }

    pub fn can_transition(self, to: Self) -> bool {
        use DownloadState::*;
        match (self, to) {
            (Queued, Probing | Paused | Cancelled) => true,
            (Probing, Downloading | Paused | Failed | Cancelled | Queued) => true,
            (Downloading, Merging | Completed | Paused | Failed | Cancelled | Queued) => true,
            (Merging, Completed | Failed | Cancelled) => true,
            (Paused, Queued | Cancelled) => true,
            (Failed, Queued | Cancelled) => true,
            _ => false,
        }
    }
}
```

`crates/odm-core/src/lib.rs`:
```rust
//! Shared pure types and helpers for ODM. No I/O lives here.
pub mod ids;
pub mod state;

pub use state::DownloadState;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p odm-core`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/odm-core
git commit -m "feat(core): identity constants and download state machine"
```

---

### Task 2: `odm-core`: error kinds

**Files:**
- Create: `crates/odm-core/src/error.rs`
- Modify: `crates/odm-core/src/lib.rs` (add `pub mod error; pub use error::{ErrorKind, OdmError};`)

**Interfaces:**
- Produces:
  - `ErrorKind` (Copy, Eq, serde snake_case): `Timeout, Connection, Dns, HttpTransient(u16), Http(u16), DiskFull, PermissionDenied, RangeIgnored, ChecksumMismatch, LinkExpired, FileChanged, Io, Cancelled, Other`
  - `ErrorKind::is_transient(&self) -> bool`
  - `ErrorKind::code(&self) -> String`, for example `"http_404"` or `"timeout"`
  - `OdmError { kind: ErrorKind, message: String }` with `OdmError::new(kind, msg)`, plus `Display` and `std::error::Error`

- [ ] **Step 1: Write the failing tests (inline in `error.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::ErrorKind::*;

    #[test]
    fn transient_classification() {
        for k in [Timeout, Connection, Dns, HttpTransient(503), HttpTransient(429)] {
            assert!(k.is_transient(), "{k:?}");
        }
        for k in [Http(404), DiskFull, PermissionDenied, RangeIgnored, ChecksumMismatch,
                  LinkExpired, FileChanged, Io, Cancelled, Other] {
            assert!(!k.is_transient(), "{k:?}");
        }
    }

    #[test]
    fn codes() {
        assert_eq!(Http(404).code(), "http_404");
        assert_eq!(HttpTransient(503).code(), "http_503");
        assert_eq!(LinkExpired.code(), "link_expired");
    }

    #[test]
    fn display_includes_message() {
        let e = super::OdmError::new(DiskFull, "no space");
        assert_eq!(e.to_string(), "disk_full: no space");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-core error`
Expected: compile error, "cannot find type `ErrorKind`".

- [ ] **Step 3: Implement**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Timeout,
    Connection,
    Dns,
    /// 408, 429, 5xx
    HttpTransient(u16),
    /// Any other non-success status
    Http(u16),
    DiskFull,
    PermissionDenied,
    RangeIgnored,
    ChecksumMismatch,
    /// 403/410 after the download had already started
    LinkExpired,
    /// Validator (ETag/Last-Modified) no longer matches
    FileChanged,
    Io,
    Cancelled,
    Other,
}

impl ErrorKind {
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Timeout | Self::Connection | Self::Dns | Self::HttpTransient(_))
    }

    pub fn code(&self) -> String {
        match self {
            Self::HttpTransient(s) | Self::Http(s) => format!("http_{s}"),
            Self::Timeout => "timeout".into(),
            Self::Connection => "connection".into(),
            Self::Dns => "dns".into(),
            Self::DiskFull => "disk_full".into(),
            Self::PermissionDenied => "permission_denied".into(),
            Self::RangeIgnored => "range_ignored".into(),
            Self::ChecksumMismatch => "checksum_mismatch".into(),
            Self::LinkExpired => "link_expired".into(),
            Self::FileChanged => "file_changed".into(),
            Self::Io => "io".into(),
            Self::Cancelled => "cancelled".into(),
            Self::Other => "other".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdmError {
    pub kind: ErrorKind,
    pub message: String,
}

impl OdmError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }
}

impl std::fmt::Display for OdmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind.code(), self.message)
    }
}

impl std::error::Error for OdmError {}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p odm-core`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/odm-core
git commit -m "feat(core): error kinds with transient classification"
```

---

### Task 3: `odm-core`: filenames and categories

**Files:**
- Create: `crates/odm-core/src/filename.rs`, `crates/odm-core/src/category.rs`
- Modify: `crates/odm-core/src/lib.rs` (add `pub mod filename; pub mod category; pub use category::Category;`)

**Interfaces:**
- Produces:
  - `filename::sanitize(name: &str) -> String`. Never returns an empty string; falls back to `"download"`.
  - `filename::from_url(url: &str) -> Option<String>`. Takes the last path segment, percent-decoded, without the query.
  - `filename::from_content_disposition(value: &str) -> Option<String>`. Prefers `filename*`. The result is sanitized.
  - `filename::unique_name(name: &str, exists: impl Fn(&str) -> bool) -> String`. Returns `name`, `name (1).ext`, …
  - `Category` (serde lowercase): `Video, Audio, Document, Compressed, Image, Program, General`, with `as_str()`, `parse(&str)`, `default_folder() -> &'static str`
  - `category::detect(filename: &str, mime: Option<&str>) -> Category`

- [ ] **Step 1: Write the failing tests**

In `filename.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_illegal_chars_and_dots() {
        assert_eq!(sanitize(r#"a<b>c:d"e/f\g|h?i*j.txt"#), "a_b_c_d_e_f_g_h_i_j.txt");
        assert_eq!(sanitize("  ..name.txt.. "), "name.txt");
        assert_eq!(sanitize("tab\tnew\nline.bin"), "tab_new_line.bin");
    }

    #[test]
    fn rejects_traversal_and_empty() {
        assert_eq!(sanitize(".."), "download");
        assert_eq!(sanitize(""), "download");
        assert_eq!(sanitize("../../etc/passwd"), "_.._etc_passwd");
    }

    #[test]
    fn reserved_windows_names_prefixed() {
        assert_eq!(sanitize("CON"), "_CON");
        assert_eq!(sanitize("nul.txt"), "_nul.txt");
        assert_eq!(sanitize("com1.log"), "_com1.log");
        assert_eq!(sanitize("console.txt"), "console.txt");
    }

    #[test]
    fn long_names_truncated_keep_extension() {
        let long = format!("{}.mkv", "a".repeat(300));
        let s = sanitize(&long);
        assert_eq!(s.chars().count(), 200);
        assert!(s.ends_with(".mkv"));
    }

    #[test]
    fn from_url_basic() {
        assert_eq!(from_url("https://x.com/a/b/file%20name.zip?token=1#f").as_deref(), Some("file name.zip"));
        assert_eq!(from_url("https://x.com/").as_deref(), None);
        assert_eq!(from_url("not a url").as_deref(), None);
    }

    #[test]
    fn disposition_plain_and_star() {
        assert_eq!(from_content_disposition(r#"attachment; filename="report.pdf""#).as_deref(), Some("report.pdf"));
        assert_eq!(from_content_disposition("attachment; filename=plain.txt").as_deref(), Some("plain.txt"));
        assert_eq!(
            from_content_disposition(r#"attachment; filename="fallback.txt"; filename*=UTF-8''na%C3%AFve%20file.txt"#).as_deref(),
            Some("naïve file.txt")
        );
        assert_eq!(from_content_disposition("inline").as_deref(), None);
    }

    #[test]
    fn long_utf8_disposition_truncated_keeps_ext() {
        let enc: String = std::iter::repeat("%E6%97%A5").take(250).collect();
        let v = format!("attachment; filename*=UTF-8''{enc}.mp4");
        let name = from_content_disposition(&v).unwrap();
        assert_eq!(name.chars().count(), 200);
        assert!(name.ends_with(".mp4"));
        assert!(name.starts_with('日'));
    }

    #[test]
    fn unique_name_increments() {
        let taken = ["x.bin", "x (1).bin"];
        assert_eq!(unique_name("x.bin", |n| taken.contains(&n)), "x (2).bin");
        assert_eq!(unique_name("y.bin", |n| taken.contains(&n)), "y.bin");
        assert_eq!(unique_name("noext", |n| n == "noext"), "noext (1)");
    }
}
```

In `category.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_extension() {
        assert_eq!(detect("a.MP4", None), Category::Video);
        assert_eq!(detect("a.flac", None), Category::Audio);
        assert_eq!(detect("a.pdf", None), Category::Document);
        assert_eq!(detect("a.tar.gz", None), Category::Compressed);
        assert_eq!(detect("a.webp", None), Category::Image);
        assert_eq!(detect("setup.msi", None), Category::Program);
        assert_eq!(detect("a.xyz", None), Category::General);
    }

    #[test]
    fn by_mime_when_no_known_ext() {
        assert_eq!(detect("stream", Some("video/mp2t")), Category::Video);
        assert_eq!(detect("x", Some("application/zip")), Category::Compressed);
        assert_eq!(detect("x", Some("application/x-msdownload")), Category::Program);
        assert_eq!(detect("a.pdf", Some("video/mp4")), Category::Document); // extension wins
    }

    #[test]
    fn folders_and_parse() {
        assert_eq!(Category::Video.default_folder(), "Videos");
        assert_eq!(Category::General.default_folder(), "");
        assert_eq!(Category::parse("program"), Some(Category::Program));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-core`
Expected: compile errors for the missing functions.

- [ ] **Step 3: Implement `filename.rs`**

```rust
use percent_encoding::percent_decode_str;

const MAX_CHARS: usize = 200;
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub fn sanitize(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = replaced.trim_matches(|c: char| c == '.' || c == ' ');
    if trimmed.is_empty() {
        return "download".into();
    }
    let mut out = trimmed.to_string();
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        out.insert(0, '_');
    }
    truncate_keep_ext(&out, MAX_CHARS)
}

fn truncate_keep_ext(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_string();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if name.len() - i <= 16 && i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let keep = max - ext.chars().count();
    let stem: String = stem.chars().take(keep).collect();
    format!("{stem}{ext}")
}

pub fn from_url(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let path = rest.split(['?', '#']).next()?;
    let (_, path) = path.split_once('/')?;
    let last = path.rsplit('/').next()?;
    if last.is_empty() {
        return None;
    }
    let decoded = percent_decode_str(last).decode_utf8_lossy();
    Some(sanitize(&decoded))
}

pub fn from_content_disposition(value: &str) -> Option<String> {
    let mut plain: Option<String> = None;
    let mut star: Option<String> = None;
    for part in value.split(';').map(str::trim) {
        let Some((k, v)) = part.split_once('=') else { continue };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim();
        if k == "filename*" {
            // charset'lang'percent-encoded
            let mut it = v.splitn(3, '\'');
            let (_charset, _lang, enc) = (it.next(), it.next(), it.next());
            if let Some(enc) = enc {
                star = Some(percent_decode_str(enc.trim_matches('"')).decode_utf8_lossy().into_owned());
            }
        } else if k == "filename" {
            plain = Some(v.trim_matches('"').to_string());
        }
    }
    star.or(plain).filter(|s| !s.is_empty()).map(|s| sanitize(&s))
}

pub fn unique_name(name: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(name) {
        return name.to_string();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (1..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !exists(candidate))
        .expect("infinite iterator")
}
```

Note: `sanitize("../../etc/passwd")` becomes `.._.._etc_passwd` after replacement, and then trimming the leading dots gives `_.._etc_passwd`. That matches the test.

- [ ] **Step 4: Implement `category.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category { Video, Audio, Document, Compressed, Image, Program, General }

impl Category {
    pub const ALL: [Category; 7] = [Self::Video, Self::Audio, Self::Document, Self::Compressed,
                                    Self::Image, Self::Program, Self::General];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Video => "video", Self::Audio => "audio", Self::Document => "document",
            Self::Compressed => "compressed", Self::Image => "image",
            Self::Program => "program", Self::General => "general",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }

    pub fn default_folder(&self) -> &'static str {
        match self {
            Self::Video => "Videos", Self::Audio => "Music", Self::Document => "Documents",
            Self::Compressed => "Compressed", Self::Image => "Images",
            Self::Program => "Programs", Self::General => "",
        }
    }
}

fn by_ext(ext: &str) -> Option<Category> {
    use Category::*;
    Some(match ext {
        "mp4" | "mkv" | "avi" | "webm" | "mov" | "flv" | "m4v" | "ts" | "m3u8" => Video,
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "wma" | "m4a" | "opus" => Audio,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "csv" | "epub" => Document,
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" | "zst" => Compressed,
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "ico" | "tiff" => Image,
        "exe" | "msi" | "msix" | "appx" => Program,
        _ => return None,
    })
}

fn by_mime(mime: &str) -> Option<Category> {
    use Category::*;
    let m = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    Some(match m.as_str() {
        s if s.starts_with("video/") => Video,
        s if s.starts_with("audio/") => Audio,
        s if s.starts_with("image/") => Image,
        "application/pdf" => Document,
        "application/zip" | "application/x-rar-compressed" | "application/vnd.rar"
        | "application/x-7z-compressed" | "application/gzip" => Compressed,
        "application/x-msdownload" | "application/x-msi" | "application/vnd.microsoft.portable-executable" => Program,
        "application/vnd.apple.mpegurl" | "application/x-mpegurl" => Video,
        _ => return None,
    })
}

pub fn detect(filename: &str, mime: Option<&str>) -> Category {
    let ext = filename.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    ext.as_deref()
        .and_then(by_ext)
        .or_else(|| mime.and_then(by_mime))
        .unwrap_or(Category::General)
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p odm-core`
Expected: all pass. If `strips_illegal_chars_and_dots` fails, check the order: replace characters first, then trim dots and spaces.

- [ ] **Step 6: Commit**

```bash
git add crates/odm-core
git commit -m "feat(core): filename sanitizing, content-disposition, categories"
```

---

### Task 4: `odm-store`: SQLite behind a single owner thread

**Files:**
- Create: `crates/odm-store/Cargo.toml`, `src/lib.rs`, `src/migrations.rs`, `src/models.rs`, `src/repo.rs`
- Test: `crates/odm-store/tests/store.rs`

**Interfaces:**
- Consumes: `odm_core::DownloadState`.
- Produces (all `pub`, re-exported from `odm_store`):
  - `DownloadRecord` with fields `id, url, final_url: Option<String>, filename, save_dir, size: Option<u64>, mime: Option<String>, category: String, state: DownloadState, error_kind: Option<String>, error_msg: Option<String>, priority: i64, max_connections: u32, speed_limit: Option<u64>, etag, last_modified, checksum: Option<String>, kind: String, page_url, page_title: Option<String>, filename_locked: bool, created_at: i64, started_at: Option<i64>, finished_at: Option<i64>`. Times are Unix ms.
    - `DownloadRecord::new_file(id, url, filename, save_dir) -> Self` sets defaults: state `Queued`, category `"general"`, `max_connections` 8, kind `"file"`, `created_at` now.
  - `ChunkRecord { idx: u32, start: u64, end: Option<u64>, written: u64 }` (inclusive `end`; `None` means an open-ended stream). The chunk's done/pending state is **derived** (`written == end-start+1`), not stored, so the stored state can never disagree with the real byte counts.
  - `ProbeMeta { final_url, size: Option<u64>, mime, etag, last_modified, filename: Option<String> }` (derives `Default`)
  - `now_ms() -> i64`
  - `Store` (Clone). Async methods, each returning `Result<_, StoreError>`:
    - `open(path: &Path)`, `open_in_memory()` (these two are sync)
    - `insert_download(DownloadRecord)`, `get_download(&str) -> Option<DownloadRecord>`
    - `list_downloads() -> Vec<DownloadRecord>` (newest first), `list_by_state(DownloadState) -> Vec<DownloadRecord>` (priority desc, then created asc)
    - `set_state(&str, DownloadState, Option<(String, String)>)`: the optional value is (error code, message). It sets `started_at` on the first `Downloading`, sets `finished_at` on a terminal state, and clears the error when `None`.
    - `update_probe(&str, ProbeMeta)`: only overwrites fields that are `Some`. The filename is applied only if `filename_locked` is false.
    - `set_filename(&str, String)`
    - `set_headers(&str, Vec<(String, String)>)` (replaces all), `get_headers(&str) -> Vec<(String, String)>`
    - `replace_chunks(&str, Vec<ChunkRecord>)` (in a transaction), `save_progress(&str, Vec<(u32, u64)>)`, `get_chunks(&str) -> Vec<ChunkRecord>` (ordered by `start`)
    - `delete_download(&str)` (cascades), `reset_interrupted() -> usize` (moves probing/downloading/merging back to queued)
    - `call(f)`: a generic escape hatch that runs `FnOnce(&mut Connection) -> rusqlite::Result<R>` on the owner thread.
  - `StoreError { Sql(rusqlite::Error), Closed }` (thiserror).

- [ ] **Step 1: Crate manifest**

`crates/odm-store/Cargo.toml`:
```toml
[package]
name = "odm-store"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
odm-core.workspace = true
rusqlite.workspace = true
tokio.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 2: Write the failing tests**

`crates/odm-store/tests/store.rs`:
```rust
use odm_core::DownloadState;
use odm_store::{ChunkRecord, DownloadRecord, ProbeMeta, Store};

fn rec(id: &str) -> DownloadRecord {
    DownloadRecord::new_file(id, "https://e.com/f.bin", "f.bin", "C:/dl")
}

#[tokio::test]
async fn fresh_db_is_migrated_and_reopen_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("odm.db");
    {
        let s = Store::open(&path).unwrap();
        let v: i64 = s.call(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0))).await.unwrap();
        assert_eq!(v, 1);
        s.insert_download(rec("a")).await.unwrap();
    }
    let s = Store::open(&path).unwrap();
    assert!(s.get_download("a").await.unwrap().is_some());
}

#[tokio::test]
async fn insert_get_roundtrip() {
    let s = Store::open_in_memory().unwrap();
    let mut r = rec("a");
    r.size = Some(123);
    r.page_title = Some("T".into());
    s.insert_download(r.clone()).await.unwrap();
    assert_eq!(s.get_download("a").await.unwrap().unwrap(), r);
    assert!(s.get_download("nope").await.unwrap().is_none());
}

#[tokio::test]
async fn set_state_tracks_timestamps_and_errors() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    s.set_state("a", DownloadState::Downloading, None).await.unwrap();
    let d = s.get_download("a").await.unwrap().unwrap();
    assert_eq!(d.state, DownloadState::Downloading);
    assert!(d.started_at.is_some() && d.finished_at.is_none());

    s.set_state("a", DownloadState::Failed, Some(("http_404".into(), "gone".into()))).await.unwrap();
    let d = s.get_download("a").await.unwrap().unwrap();
    assert_eq!(d.error_kind.as_deref(), Some("http_404"));
    assert!(d.finished_at.is_some());

    s.set_state("a", DownloadState::Queued, None).await.unwrap();
    let d = s.get_download("a").await.unwrap().unwrap();
    assert!(d.error_kind.is_none() && d.error_msg.is_none() && d.finished_at.is_none());
}

#[tokio::test]
async fn probe_respects_filename_lock() {
    let s = Store::open_in_memory().unwrap();
    let mut locked = rec("a");
    locked.filename_locked = true;
    s.insert_download(locked).await.unwrap();
    s.insert_download(rec("b")).await.unwrap();
    let meta = ProbeMeta { size: Some(10), filename: Some("server.zip".into()), ..Default::default() };
    s.update_probe("a", meta.clone()).await.unwrap();
    s.update_probe("b", meta).await.unwrap();
    assert_eq!(s.get_download("a").await.unwrap().unwrap().filename, "f.bin");
    let b = s.get_download("b").await.unwrap().unwrap();
    assert_eq!((b.filename.as_str(), b.size), ("server.zip", Some(10)));
}

#[tokio::test]
async fn headers_replace() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    s.set_headers("a", vec![("Cookie".into(), "x=1".into()), ("Referer".into(), "r".into())]).await.unwrap();
    s.set_headers("a", vec![("Cookie".into(), "x=2".into())]).await.unwrap();
    assert_eq!(s.get_headers("a").await.unwrap(), vec![("Cookie".to_string(), "x=2".to_string())]);
}

#[tokio::test]
async fn chunks_replace_and_progress() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    let chunks = vec![
        ChunkRecord { idx: 0, start: 0, end: Some(99), written: 0 },
        ChunkRecord { idx: 1, start: 100, end: None, written: 0 },
    ];
    s.replace_chunks("a", chunks).await.unwrap();
    s.save_progress("a", vec![(0, 50), (1, 7)]).await.unwrap();
    let got = s.get_chunks("a").await.unwrap();
    assert_eq!(got[0], ChunkRecord { idx: 0, start: 0, end: Some(99), written: 50 });
    assert_eq!(got[1], ChunkRecord { idx: 1, start: 100, end: None, written: 7 });
    s.replace_chunks("a", vec![ChunkRecord { idx: 0, start: 0, end: Some(9), written: 0 }]).await.unwrap();
    assert_eq!(s.get_chunks("a").await.unwrap().len(), 1);
}

#[tokio::test]
async fn delete_cascades_and_reset_interrupted() {
    let s = Store::open_in_memory().unwrap();
    for id in ["a", "b", "c"] {
        s.insert_download(rec(id)).await.unwrap();
    }
    s.set_headers("a", vec![("A".into(), "1".into())]).await.unwrap();
    s.replace_chunks("a", vec![ChunkRecord { idx: 0, start: 0, end: None, written: 0 }]).await.unwrap();
    s.delete_download("a").await.unwrap();
    let n: i64 = s.call(|c| c.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))).await.unwrap();
    assert_eq!(n, 0);

    s.set_state("b", DownloadState::Downloading, None).await.unwrap();
    s.set_state("c", DownloadState::Paused, None).await.unwrap();
    assert_eq!(s.reset_interrupted().await.unwrap(), 1);
    assert_eq!(s.get_download("b").await.unwrap().unwrap().state, DownloadState::Queued);
    assert_eq!(s.get_download("c").await.unwrap().unwrap().state, DownloadState::Paused);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_callers_are_serialized() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    let chunks = (0..16u32)
        .map(|i| ChunkRecord { idx: i, start: i as u64 * 10, end: Some(i as u64 * 10 + 9), written: 0 })
        .collect();
    s.replace_chunks("a", chunks).await.unwrap();
    let mut tasks = Vec::new();
    for i in 0..16u32 {
        let s = s.clone();
        tasks.push(tokio::spawn(async move {
            for w in 1..=10u64 {
                s.save_progress("a", vec![(i, w)]).await.unwrap();
                s.get_chunks("a").await.unwrap();
            }
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    assert!(s.get_chunks("a").await.unwrap().iter().all(|c| c.written == 10));
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p odm-store`
Expected: compile errors (there's no `lib.rs` yet).

- [ ] **Step 4: Implement `src/migrations.rs`**

```rust
use rusqlite::Connection;

const V1: &str = r#"
CREATE TABLE downloads (
    id              TEXT PRIMARY KEY,
    url             TEXT NOT NULL,
    final_url       TEXT,
    filename        TEXT NOT NULL,
    save_dir        TEXT NOT NULL,
    size            INTEGER,
    mime            TEXT,
    category        TEXT NOT NULL DEFAULT 'general',
    state           TEXT NOT NULL,
    error_kind      TEXT,
    error_msg       TEXT,
    priority        INTEGER NOT NULL DEFAULT 0,
    max_connections INTEGER NOT NULL DEFAULT 8,
    speed_limit     INTEGER,
    etag            TEXT,
    last_modified   TEXT,
    checksum        TEXT,
    kind            TEXT NOT NULL DEFAULT 'file',
    page_url        TEXT,
    page_title      TEXT,
    filename_locked INTEGER NOT NULL DEFAULT 0,
    created_at      INTEGER NOT NULL,
    started_at      INTEGER,
    finished_at     INTEGER
);
CREATE INDEX idx_downloads_state ON downloads(state);

CREATE TABLE download_headers (
    download_id TEXT NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    value       TEXT NOT NULL,
    PRIMARY KEY (download_id, name)
);

CREATE TABLE chunks (
    download_id TEXT NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    start       INTEGER NOT NULL,
    end_incl    INTEGER,
    written     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (download_id, idx)
);

CREATE TABLE hls_segments (
    download_id TEXT NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    url         TEXT NOT NULL,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    state       TEXT NOT NULL DEFAULT 'queued',
    bytes       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (download_id, idx)
);

CREATE TABLE hls_meta (
    download_id TEXT PRIMARY KEY REFERENCES downloads(id) ON DELETE CASCADE,
    variant_url TEXT NOT NULL,
    key_url     TEXT,
    iv          TEXT,
    map_url     TEXT
);
"#;

/// Ordered; index i migrates to user_version i+1. Append only — never edit a shipped entry.
const MIGRATIONS: &[&str] = &[V1];

pub fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}
```

- [ ] **Step 5: Implement `src/models.rs`**

```rust
use odm_core::DownloadState;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadRecord {
    pub id: String,
    pub url: String,
    pub final_url: Option<String>,
    pub filename: String,
    pub save_dir: String,
    pub size: Option<u64>,
    pub mime: Option<String>,
    pub category: String,
    pub state: DownloadState,
    pub error_kind: Option<String>,
    pub error_msg: Option<String>,
    pub priority: i64,
    pub max_connections: u32,
    pub speed_limit: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub checksum: Option<String>,
    pub kind: String,
    pub page_url: Option<String>,
    pub page_title: Option<String>,
    pub filename_locked: bool,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

impl DownloadRecord {
    pub fn new_file(id: &str, url: &str, filename: &str, save_dir: &str) -> Self {
        Self {
            id: id.into(), url: url.into(), final_url: None, filename: filename.into(),
            save_dir: save_dir.into(), size: None, mime: None, category: "general".into(),
            state: DownloadState::Queued, error_kind: None, error_msg: None, priority: 0,
            max_connections: 8, speed_limit: None, etag: None, last_modified: None,
            checksum: None, kind: "file".into(), page_url: None, page_title: None,
            filename_locked: false, created_at: now_ms(), started_at: None, finished_at: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkRecord {
    pub idx: u32,
    pub start: u64,
    /// Inclusive end; None = open-ended stream of unknown length.
    pub end: Option<u64>,
    pub written: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProbeMeta {
    pub final_url: Option<String>,
    pub size: Option<u64>,
    pub mime: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub filename: Option<String>,
}
```

- [ ] **Step 6: Implement `src/repo.rs`**

```rust
use odm_core::DownloadState;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::models::{now_ms, ChunkRecord, DownloadRecord, ProbeMeta};

const COLS: &str = "id,url,final_url,filename,save_dir,size,mime,category,state,error_kind,error_msg,\
priority,max_connections,speed_limit,etag,last_modified,checksum,kind,page_url,page_title,\
filename_locked,created_at,started_at,finished_at";

fn to_record(r: &Row) -> rusqlite::Result<DownloadRecord> {
    let state: String = r.get(8)?;
    Ok(DownloadRecord {
        id: r.get(0)?,
        url: r.get(1)?,
        final_url: r.get(2)?,
        filename: r.get(3)?,
        save_dir: r.get(4)?,
        size: r.get::<_, Option<i64>>(5)?.map(|v| v as u64),
        mime: r.get(6)?,
        category: r.get(7)?,
        state: DownloadState::parse(&state).unwrap_or(DownloadState::Failed),
        error_kind: r.get(9)?,
        error_msg: r.get(10)?,
        priority: r.get(11)?,
        max_connections: r.get::<_, i64>(12)? as u32,
        speed_limit: r.get::<_, Option<i64>>(13)?.map(|v| v as u64),
        etag: r.get(14)?,
        last_modified: r.get(15)?,
        checksum: r.get(16)?,
        kind: r.get(17)?,
        page_url: r.get(18)?,
        page_title: r.get(19)?,
        filename_locked: r.get::<_, i64>(20)? != 0,
        created_at: r.get(21)?,
        started_at: r.get(22)?,
        finished_at: r.get(23)?,
    })
}

pub fn insert_download(c: &Connection, d: &DownloadRecord) -> rusqlite::Result<()> {
    c.execute(
        &format!(
            "INSERT INTO downloads ({COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24)"
        ),
        params![
            d.id, d.url, d.final_url, d.filename, d.save_dir, d.size.map(|v| v as i64), d.mime,
            d.category, d.state.as_str(), d.error_kind, d.error_msg, d.priority,
            d.max_connections as i64, d.speed_limit.map(|v| v as i64), d.etag, d.last_modified,
            d.checksum, d.kind, d.page_url, d.page_title, d.filename_locked as i64, d.created_at,
            d.started_at, d.finished_at
        ],
    )?;
    Ok(())
}

pub fn get_download(c: &Connection, id: &str) -> rusqlite::Result<Option<DownloadRecord>> {
    c.query_row(&format!("SELECT {COLS} FROM downloads WHERE id=?1"), [id], to_record).optional()
}

pub fn list_downloads(c: &Connection) -> rusqlite::Result<Vec<DownloadRecord>> {
    let mut st = c.prepare(&format!("SELECT {COLS} FROM downloads ORDER BY created_at DESC, rowid DESC"))?;
    let rows = st.query_map([], to_record)?;
    rows.collect()
}

pub fn list_by_state(c: &Connection, s: DownloadState) -> rusqlite::Result<Vec<DownloadRecord>> {
    let mut st = c.prepare(&format!(
        "SELECT {COLS} FROM downloads WHERE state=?1 ORDER BY priority DESC, created_at ASC, rowid ASC"
    ))?;
    let rows = st.query_map([s.as_str()], to_record)?;
    rows.collect()
}

pub fn set_state(c: &Connection, id: &str, s: DownloadState, err: Option<(String, String)>) -> rusqlite::Result<()> {
    let (kind, msg) = match err {
        Some((k, m)) => (Some(k), Some(m)),
        None => (None, None),
    };
    c.execute(
        "UPDATE downloads SET state=?2, error_kind=?3, error_msg=?4,
            started_at = CASE WHEN ?2='downloading' AND started_at IS NULL THEN ?5 ELSE started_at END,
            finished_at = CASE WHEN ?2 IN ('completed','failed','cancelled') THEN ?5
                               WHEN ?2='queued' THEN NULL ELSE finished_at END
         WHERE id=?1",
        params![id, s.as_str(), kind, msg, now_ms()],
    )?;
    Ok(())
}

pub fn update_probe(c: &Connection, id: &str, m: &ProbeMeta) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE downloads SET
            final_url = COALESCE(?2, final_url),
            size = COALESCE(?3, size),
            mime = COALESCE(?4, mime),
            etag = COALESCE(?5, etag),
            last_modified = COALESCE(?6, last_modified),
            filename = CASE WHEN filename_locked = 0 AND ?7 IS NOT NULL THEN ?7 ELSE filename END
         WHERE id=?1",
        params![id, m.final_url, m.size.map(|v| v as i64), m.mime, m.etag, m.last_modified, m.filename],
    )?;
    Ok(())
}

pub fn set_filename(c: &Connection, id: &str, name: &str) -> rusqlite::Result<()> {
    c.execute("UPDATE downloads SET filename=?2 WHERE id=?1", params![id, name])?;
    Ok(())
}

pub fn set_headers(c: &mut Connection, id: &str, h: &[(String, String)]) -> rusqlite::Result<()> {
    let tx = c.transaction()?;
    tx.execute("DELETE FROM download_headers WHERE download_id=?1", [id])?;
    for (n, v) in h {
        tx.execute(
            "INSERT OR REPLACE INTO download_headers (download_id,name,value) VALUES (?1,?2,?3)",
            params![id, n, v],
        )?;
    }
    tx.commit()
}

pub fn get_headers(c: &Connection, id: &str) -> rusqlite::Result<Vec<(String, String)>> {
    let mut st = c.prepare("SELECT name,value FROM download_headers WHERE download_id=?1 ORDER BY rowid")?;
    let rows = st.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

pub fn replace_chunks(c: &mut Connection, id: &str, chunks: &[ChunkRecord]) -> rusqlite::Result<()> {
    let tx = c.transaction()?;
    tx.execute("DELETE FROM chunks WHERE download_id=?1", [id])?;
    for ch in chunks {
        tx.execute(
            "INSERT INTO chunks (download_id,idx,start,end_incl,written) VALUES (?1,?2,?3,?4,?5)",
            params![id, ch.idx, ch.start as i64, ch.end.map(|v| v as i64), ch.written as i64],
        )?;
    }
    tx.commit()
}

pub fn save_progress(c: &mut Connection, id: &str, p: &[(u32, u64)]) -> rusqlite::Result<()> {
    let tx = c.transaction()?;
    for (idx, w) in p {
        tx.execute(
            "UPDATE chunks SET written=?3 WHERE download_id=?1 AND idx=?2",
            params![id, idx, *w as i64],
        )?;
    }
    tx.commit()
}

pub fn get_chunks(c: &Connection, id: &str) -> rusqlite::Result<Vec<ChunkRecord>> {
    let mut st = c.prepare("SELECT idx,start,end_incl,written FROM chunks WHERE download_id=?1 ORDER BY start")?;
    let rows = st.query_map([id], |r| {
        Ok(ChunkRecord {
            idx: r.get::<_, i64>(0)? as u32,
            start: r.get::<_, i64>(1)? as u64,
            end: r.get::<_, Option<i64>>(2)?.map(|v| v as u64),
            written: r.get::<_, i64>(3)? as u64,
        })
    })?;
    rows.collect()
}

pub fn delete_download(c: &Connection, id: &str) -> rusqlite::Result<()> {
    c.execute("DELETE FROM downloads WHERE id=?1", [id])?;
    Ok(())
}

pub fn reset_interrupted(c: &Connection) -> rusqlite::Result<usize> {
    c.execute(
        "UPDATE downloads SET state='queued' WHERE state IN ('probing','downloading','merging')",
        [],
    )
}
```

- [ ] **Step 7: Implement `src/lib.rs`**

```rust
//! SQLite persistence. One dedicated thread owns the connection; callers
//! submit closures over a channel and await the result.
mod migrations;
mod models;
mod repo;

pub use models::{now_ms, ChunkRecord, DownloadRecord, ProbeMeta};

use odm_core::DownloadState;
use rusqlite::Connection;
use std::path::Path;
use std::sync::mpsc;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("store thread closed")]
    Closed,
}

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

#[derive(Clone)]
pub struct Store {
    tx: mpsc::Sender<Job>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self::start(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::start(Connection::open_in_memory()?)
    }

    fn start(mut conn: Connection) -> Result<Self, StoreError> {
        // In-memory DBs answer "memory" to the WAL pragma; that's fine.
        let _: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        migrations::migrate(&mut conn)?;
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("odm-store".into())
            .spawn(move || {
                for job in rx {
                    job(&mut conn);
                }
            })
            .expect("spawn store thread");
        Ok(Self { tx })
    }

    pub async fn call<R, F>(&self, f: F) -> Result<R, StoreError>
    where
        R: Send + 'static,
        F: FnOnce(&mut Connection) -> rusqlite::Result<R> + Send + 'static,
    {
        let (otx, orx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Box::new(move |c| {
                let _ = otx.send(f(c));
            }))
            .map_err(|_| StoreError::Closed)?;
        Ok(orx.await.map_err(|_| StoreError::Closed)??)
    }

    pub async fn insert_download(&self, d: DownloadRecord) -> Result<(), StoreError> {
        self.call(move |c| repo::insert_download(c, &d)).await
    }
    pub async fn get_download(&self, id: &str) -> Result<Option<DownloadRecord>, StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::get_download(c, &id)).await
    }
    pub async fn list_downloads(&self) -> Result<Vec<DownloadRecord>, StoreError> {
        self.call(|c| repo::list_downloads(c)).await
    }
    pub async fn list_by_state(&self, s: DownloadState) -> Result<Vec<DownloadRecord>, StoreError> {
        self.call(move |c| repo::list_by_state(c, s)).await
    }
    pub async fn set_state(&self, id: &str, s: DownloadState, err: Option<(String, String)>) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::set_state(c, &id, s, err)).await
    }
    pub async fn update_probe(&self, id: &str, m: ProbeMeta) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::update_probe(c, &id, &m)).await
    }
    pub async fn set_filename(&self, id: &str, name: String) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::set_filename(c, &id, &name)).await
    }
    pub async fn set_headers(&self, id: &str, h: Vec<(String, String)>) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::set_headers(c, &id, &h)).await
    }
    pub async fn get_headers(&self, id: &str) -> Result<Vec<(String, String)>, StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::get_headers(c, &id)).await
    }
    pub async fn replace_chunks(&self, id: &str, ch: Vec<ChunkRecord>) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::replace_chunks(c, &id, &ch)).await
    }
    pub async fn save_progress(&self, id: &str, p: Vec<(u32, u64)>) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::save_progress(c, &id, &p)).await
    }
    pub async fn get_chunks(&self, id: &str) -> Result<Vec<ChunkRecord>, StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::get_chunks(c, &id)).await
    }
    pub async fn delete_download(&self, id: &str) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::delete_download(c, &id)).await
    }
    pub async fn reset_interrupted(&self) -> Result<usize, StoreError> {
        self.call(|c| repo::reset_interrupted(c)).await
    }
}
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test -p odm-store`
Expected: 8 passed.

- [ ] **Step 9: Commit**

```bash
git add crates/odm-store
git commit -m "feat(store): sqlite store on owner thread with migrations and repo"
```

---

### Task 5: `odm-engine` crate: speed limiter

**Files:**
- Create: `crates/odm-engine/Cargo.toml`, `crates/odm-engine/src/lib.rs`, `crates/odm-engine/src/limiter.rs`
- Test: `crates/odm-engine/tests/limiter.rs`

**Interfaces:**
- Produces:
  - `odm_engine::SpeedLimiter::new(rate_bps: u64) -> SpeedLimiter` (0 means unlimited)
  - `.set_rate(&self, bps: u64)`, `.rate(&self) -> u64`
  - `async .acquire(&self, n: usize)`. It never spins. It waits at most (n + existing debt) / rate seconds.

- [ ] **Step 1: Crate manifest and lib**

`crates/odm-engine/Cargo.toml`:
```toml
[package]
name = "odm-engine"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
odm-core.workspace = true
odm-store.workspace = true
tokio.workspace = true
tokio-util.workspace = true
reqwest.workspace = true
futures-util.workspace = true
fastrand.workspace = true
tracing.workspace = true
uuid.workspace = true
bytes.workspace = true
serde.workspace = true

[dev-dependencies]
# odm-testserver.workspace = true   # uncomment in Task 8
tempfile.workspace = true
sha2.workspace = true
```

`crates/odm-engine/src/lib.rs`:
```rust
//! ODM download engine.
pub mod limiter;

pub use limiter::SpeedLimiter;
```

- [ ] **Step 2: Write the failing tests**

`crates/odm-engine/tests/limiter.rs`:
```rust
use odm_engine::SpeedLimiter;
use std::time::{Duration, Instant};

#[tokio::test]
async fn unlimited_is_instant() {
    let l = SpeedLimiter::new(0);
    let t = Instant::now();
    l.acquire(100_000_000).await;
    assert!(t.elapsed() < Duration::from_millis(50));
}

#[tokio::test]
async fn request_larger_than_bucket_completes() {
    // Regression: the old limiter looped forever here.
    let l = SpeedLimiter::new(10_000);
    let t = Instant::now();
    l.acquire(15_000).await; // 10k burst, 5k debt -> ~0.5 s
    let e = t.elapsed();
    assert!(e >= Duration::from_millis(350) && e < Duration::from_secs(2), "{e:?}");
}

#[tokio::test]
async fn tiny_limit_below_buffer_size_does_not_hang() {
    let l = SpeedLimiter::new(4_096);
    let t = Instant::now();
    l.acquire(8_192).await; // 4k burst, 4k debt -> ~1 s
    assert!(t.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn sustained_rate_is_respected() {
    let l = SpeedLimiter::new(20_000);
    let t = Instant::now();
    for _ in 0..10 {
        l.acquire(5_000).await; // 50k total, 20k burst -> ~1.5 s
    }
    let e = t.elapsed();
    assert!(e >= Duration::from_millis(1_200) && e < Duration::from_millis(2_500), "{e:?}");
}

#[tokio::test]
async fn switching_to_unlimited_takes_effect() {
    let l = SpeedLimiter::new(1_000);
    l.set_rate(0);
    assert_eq!(l.rate(), 0);
    let t = Instant::now();
    l.acquire(1_000_000).await;
    assert!(t.elapsed() < Duration::from_millis(50));
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p odm-engine --test limiter`
Expected: compile error, "file not found for module `limiter`".

- [ ] **Step 4: Implement `src/limiter.rs`**

```rust
//! Token bucket that allows debt, so a request larger than the bucket
//! waits proportionally instead of spinning forever.
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Bucket {
    rate: u64,
    tokens: f64,
    last: Instant,
}

pub struct SpeedLimiter {
    inner: Mutex<Bucket>,
}

impl SpeedLimiter {
    pub fn new(rate_bps: u64) -> Self {
        Self { inner: Mutex::new(Bucket { rate: rate_bps, tokens: rate_bps as f64, last: Instant::now() }) }
    }

    pub fn rate(&self) -> u64 {
        self.inner.lock().unwrap().rate
    }

    pub fn set_rate(&self, bps: u64) {
        let mut b = self.inner.lock().unwrap();
        b.rate = bps;
        b.tokens = bps as f64;
        b.last = Instant::now();
    }

    pub async fn acquire(&self, n: usize) {
        let wait = {
            let mut b = self.inner.lock().unwrap();
            if b.rate == 0 {
                return;
            }
            let now = Instant::now();
            let rate = b.rate as f64;
            b.tokens = (b.tokens + now.duration_since(b.last).as_secs_f64() * rate).min(rate);
            b.last = now;
            b.tokens -= n as f64;
            if b.tokens < 0.0 { Duration::from_secs_f64(-b.tokens / rate) } else { Duration::ZERO }
        }; // lock released before sleeping
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p odm-engine --test limiter`
Expected: 5 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/odm-engine
git commit -m "feat(engine): debt-based token bucket speed limiter"
```

---

### Task 6: `odm-engine`: retry backoff and error classification

**Files:**
- Create: `crates/odm-engine/src/retry.rs`
- Modify: `crates/odm-engine/src/lib.rs` (add `pub mod retry;`)

**Interfaces:**
- Produces:
  - `retry::backoff_delay(attempt: u32, retry_after: Option<Duration>) -> Duration` (random jitter)
  - `retry::backoff_with_jitter(attempt: u32, retry_after: Option<Duration>, jitter: f64) -> Duration` (deterministic; jitter is clamped to [-0.2, 0.2]; attempt is 1-based)
  - `retry::parse_retry_after(v: &str) -> Option<Duration>` (whole seconds; capped at 300 s)
  - `retry::classify_status(status: u16, started: bool) -> ErrorKind`
  - `retry::classify_io(e: &std::io::Error) -> ErrorKind`
  - `retry::classify_reqwest(e: &reqwest::Error) -> ErrorKind`
  - `retry::io_err(e: std::io::Error) -> OdmError`, `retry::http_err(e: reqwest::Error) -> OdmError` (the message never contains the URL)

- [ ] **Step 1: Write the failing tests (at the bottom of `retry.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use odm_core::ErrorKind::*;

    #[test]
    fn exponential_with_cap() {
        let d = |a| backoff_with_jitter(a, None, 0.0).as_secs_f64();
        assert_eq!([d(1), d(2), d(3), d(4)], [1.0, 2.0, 4.0, 8.0]);
        assert_eq!(d(7), 60.0);
        assert_eq!(d(30), 60.0);
    }

    #[test]
    fn jitter_bounds() {
        assert_eq!(backoff_with_jitter(2, None, 0.2).as_secs_f64(), 2.4);
        assert_eq!(backoff_with_jitter(2, None, -0.2).as_secs_f64(), 1.6);
        for _ in 0..100 {
            let s = backoff_delay(3, None).as_secs_f64();
            assert!((3.2..=4.8).contains(&s), "{s}");
        }
    }

    #[test]
    fn retry_after_wins_and_is_capped() {
        assert_eq!(backoff_with_jitter(1, Some(Duration::from_secs(7)), 0.2), Duration::from_secs(7));
        assert_eq!(parse_retry_after("12"), Some(Duration::from_secs(12)));
        assert_eq!(parse_retry_after("100000"), Some(Duration::from_secs(300)));
        assert_eq!(parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT"), None);
    }

    #[test]
    fn status_classification() {
        assert_eq!(classify_status(503, false), HttpTransient(503));
        assert_eq!(classify_status(429, true), HttpTransient(429));
        assert_eq!(classify_status(408, false), HttpTransient(408));
        assert_eq!(classify_status(404, true), Http(404));
        assert_eq!(classify_status(403, false), Http(403));
        assert_eq!(classify_status(403, true), LinkExpired);
        assert_eq!(classify_status(410, true), LinkExpired);
    }

    #[test]
    fn io_classification() {
        use std::io::{Error, ErrorKind as K};
        assert_eq!(classify_io(&Error::from(K::PermissionDenied)), PermissionDenied);
        assert_eq!(classify_io(&Error::from(K::StorageFull)), DiskFull);
        assert_eq!(classify_io(&Error::from_raw_os_error(112)), DiskFull); // ERROR_DISK_FULL on Windows
        assert_eq!(classify_io(&Error::from(K::NotFound)), Io);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-engine --lib retry`
Expected: compile error (no functions yet).

- [ ] **Step 3: Implement (top of `retry.rs`)**

```rust
use odm_core::{ErrorKind, OdmError};
use std::time::Duration;

const CAP_SECS: f64 = 60.0;
const RETRY_AFTER_CAP: u64 = 300;

pub fn backoff_with_jitter(attempt: u32, retry_after: Option<Duration>, jitter: f64) -> Duration {
    if let Some(ra) = retry_after {
        return ra.min(Duration::from_secs(RETRY_AFTER_CAP));
    }
    let exp = attempt.max(1).saturating_sub(1).min(10) as i32;
    let base = 2f64.powi(exp).min(CAP_SECS);
    let secs = (base * (1.0 + jitter.clamp(-0.2, 0.2)) * 1000.0).round() / 1000.0;
    Duration::from_secs_f64(secs)
}

pub fn backoff_delay(attempt: u32, retry_after: Option<Duration>) -> Duration {
    backoff_with_jitter(attempt, retry_after, fastrand::f64() * 0.4 - 0.2)
}

pub fn parse_retry_after(v: &str) -> Option<Duration> {
    v.trim().parse::<u64>().ok().map(|s| Duration::from_secs(s.min(RETRY_AFTER_CAP)))
}

pub fn classify_status(status: u16, started: bool) -> ErrorKind {
    match status {
        408 | 429 | 500..=599 => ErrorKind::HttpTransient(status),
        403 | 410 if started => ErrorKind::LinkExpired,
        s => ErrorKind::Http(s),
    }
}

pub fn classify_io(e: &std::io::Error) -> ErrorKind {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::PermissionDenied => ErrorKind::PermissionDenied,
        K::StorageFull => ErrorKind::DiskFull,
        // 112 = ERROR_DISK_FULL, 39 = ERROR_HANDLE_DISK_FULL
        _ if matches!(e.raw_os_error(), Some(112) | Some(39)) => ErrorKind::DiskFull,
        _ => ErrorKind::Io,
    }
}

pub fn classify_reqwest(e: &reqwest::Error) -> ErrorKind {
    if e.is_timeout() {
        ErrorKind::Timeout
    } else if let Some(s) = e.status() {
        classify_status(s.as_u16(), false)
    } else if e.is_connect() || e.is_body() || e.is_request() || e.is_decode() {
        ErrorKind::Connection
    } else {
        ErrorKind::Other
    }
}

pub fn io_err(e: std::io::Error) -> OdmError {
    OdmError::new(classify_io(&e), e.to_string())
}

pub fn http_err(e: reqwest::Error) -> OdmError {
    let kind = classify_reqwest(&e);
    OdmError::new(kind, e.without_url().to_string())
}
```

Note: Windows sometimes reports a disk-full condition with an `ErrorKind` other than `StorageFull`, which is why the raw OS error codes are checked too. `without_url()` removes the URL (and its query string) from the message.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p odm-engine --lib retry`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/odm-engine
git commit -m "feat(engine): backoff and error classification"
```

---

### Task 7: `odm-engine`: chunk table (planning, claiming, splitting)

This is the core of correct parallel writing. A chunk's claimed position only moves forward while its mutex is held. A split cuts at a point past the claimed position, so two workers can never write the same bytes.

**Files:**
- Create: `crates/odm-engine/src/chunks.rs`
- Modify: `crates/odm-engine/src/lib.rs` (add `pub mod chunks;`)

**Interfaces:**
- Consumes: `odm_store::ChunkRecord`
- Produces:
  - `pub const MIN_CHUNK: u64 = 1 << 20;` and `pub const MIN_SPLIT_REMAINING: u64 = 2 << 20;`
  - `pub fn plan_chunks(size: Option<u64>, supports_range: bool, max_connections: u32) -> Vec<ChunkRecord>`
  - `pub struct ChunkShared { pub idx: u32, pub start: u64, pub end: Option<u64>, pub claimed: u64, pub written: u64 }` with:
    - `remaining(&self) -> Option<u64>` (from the claimed position to the end)
    - `is_done(&self) -> bool`
    - `claim(&mut self, want: usize) -> (u64 /*file offset*/, usize /*allowed*/)`
  - `pub type ChunkRef = std::sync::Arc<std::sync::Mutex<ChunkShared>>;`
  - `pub struct ChunkTable` with:
    - `from_records(&[ChunkRecord]) -> Self` (claimed starts at written)
    - `refs(&self) -> Vec<ChunkRef>`
    - `pending(&self) -> Vec<ChunkRef>` (not done)
    - `split_largest(&mut self) -> Option<ChunkRef>` (returns the new chunk)
    - `snapshot(&self) -> Vec<ChunkRecord>` (uses `written`, never `claimed`; ordered by start)
    - `progress(&self) -> Vec<(u32, u64)>`
    - `total_written(&self) -> u64`

- [ ] **Step 1: Write the failing tests (bottom of `chunks.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    const MB: u64 = 1 << 20;

    #[test]
    fn plan_unknown_size_is_single_open_chunk() {
        let p = plan_chunks(None, true, 8);
        assert_eq!(p, vec![ChunkRecord { idx: 0, start: 0, end: None, written: 0 }]);
    }

    #[test]
    fn plan_no_range_is_single_chunk() {
        let p = plan_chunks(Some(100 * MB), false, 8);
        assert_eq!(p, vec![ChunkRecord { idx: 0, start: 0, end: Some(100 * MB - 1), written: 0 }]);
    }

    #[test]
    fn plan_zero_size_is_empty() {
        assert!(plan_chunks(Some(0), true, 8).is_empty());
    }

    #[test]
    fn plan_respects_min_chunk_and_max_conn() {
        assert_eq!(plan_chunks(Some(3 * MB), true, 8).len(), 3);
        assert_eq!(plan_chunks(Some(500 * MB), true, 8).len(), 8);
        assert_eq!(plan_chunks(Some(10), true, 8).len(), 1);
    }

    #[test]
    fn plan_covers_every_byte_exactly_once() {
        let size = 10 * MB + 12345;
        let p = plan_chunks(Some(size), true, 7);
        assert_eq!(p[0].start, 0);
        for w in p.windows(2) {
            assert_eq!(w[0].end.unwrap() + 1, w[1].start);
        }
        assert_eq!(p.last().unwrap().end, Some(size - 1));
    }

    #[test]
    fn claim_stops_at_end() {
        let mut c = ChunkShared { idx: 0, start: 100, end: Some(109), claimed: 0, written: 0 };
        assert_eq!(c.claim(4), (100, 4));
        assert_eq!(c.claim(100), (104, 6));
        assert_eq!(c.claim(8), (110, 0));
        assert_eq!(c.remaining(), Some(0));
    }

    #[test]
    fn claim_open_ended_is_unbounded() {
        let mut c = ChunkShared { idx: 0, start: 0, end: None, claimed: 0, written: 0 };
        assert_eq!(c.claim(8192), (0, 8192));
        assert_eq!(c.remaining(), None);
        assert!(!c.is_done());
    }

    #[test]
    fn split_takes_upper_half_of_remaining() {
        let recs = vec![
            ChunkRecord { idx: 0, start: 0, end: Some(10 * MB - 1), written: 0 },
            ChunkRecord { idx: 1, start: 10 * MB, end: Some(11 * MB - 1), written: 0 },
        ];
        let mut t = ChunkTable::from_records(&recs);
        {
            let r = t.refs();
            r[0].lock().unwrap().claim((2 * MB) as usize); // worker advanced 2 MB
        }
        let new = t.split_largest().expect("split");
        let n = new.lock().unwrap();
        // remaining of chunk 0 is [2MB, 10MB-1] = 8MB -> mid at 6MB
        assert_eq!((n.idx, n.start, n.end), (2, 6 * MB, Some(10 * MB - 1)));
        assert_eq!(t.refs()[0].lock().unwrap().end, Some(6 * MB - 1));
    }

    #[test]
    fn split_refuses_small_or_open_chunks() {
        // Remaining exactly 2 MiB -> not strictly greater than MIN_SPLIT_REMAINING -> no split.
        let mut t = ChunkTable::from_records(&[ChunkRecord { idx: 0, start: 0, end: Some(2 * MB - 1), written: 0 }]);
        assert!(t.split_largest().is_none());
        let mut t = ChunkTable::from_records(&[ChunkRecord { idx: 0, start: 0, end: None, written: 0 }]);
        assert!(t.split_largest().is_none());
    }

    #[test]
    fn snapshot_uses_written_not_claimed() {
        let mut t = ChunkTable::from_records(&[ChunkRecord { idx: 0, start: 0, end: Some(99), written: 10 }]);
        {
            let r = t.refs();
            let mut c = r[0].lock().unwrap();
            assert_eq!(c.claim(50), (10, 50)); // resumes at written
            c.written += 20; // only 20 of the 50 claimed bytes flushed
        }
        assert_eq!(t.snapshot()[0].written, 30);
        assert_eq!(t.total_written(), 30);
        assert!(t.split_largest().is_none());
    }
}
```

**Split rule:** a chunk is split only if its remaining bytes (end + 1 − start − claimed) are **strictly greater than** `MIN_SPLIT_REMAINING` (2 MiB). Open-ended chunks are never split.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-engine --lib chunks`
Expected: compile error.

- [ ] **Step 3: Implement (top of `chunks.rs`)**

```rust
use odm_store::ChunkRecord;
use std::sync::{Arc, Mutex};

pub const MIN_CHUNK: u64 = 1 << 20;
pub const MIN_SPLIT_REMAINING: u64 = 2 << 20;

pub fn plan_chunks(size: Option<u64>, supports_range: bool, max_connections: u32) -> Vec<ChunkRecord> {
    match size {
        Some(0) => Vec::new(),
        Some(size) if supports_range => {
            let n = (size / MIN_CHUNK).clamp(1, max_connections.max(1) as u64);
            let base = size / n;
            (0..n)
                .map(|i| {
                    let start = i * base;
                    let end = if i == n - 1 { size - 1 } else { start + base - 1 };
                    ChunkRecord { idx: i as u32, start, end: Some(end), written: 0 }
                })
                .collect()
        }
        Some(size) => vec![ChunkRecord { idx: 0, start: 0, end: Some(size - 1), written: 0 }],
        None => vec![ChunkRecord { idx: 0, start: 0, end: None, written: 0 }],
    }
}

#[derive(Debug, Clone)]
pub struct ChunkShared {
    pub idx: u32,
    pub start: u64,
    pub end: Option<u64>,
    /// Bytes reserved by a worker (>= written).
    pub claimed: u64,
    /// Bytes written AND flushed-eligible; the only value persisted.
    pub written: u64,
}

impl ChunkShared {
    pub fn remaining(&self) -> Option<u64> {
        self.end.map(|e| (e + 1).saturating_sub(self.start + self.claimed))
    }

    pub fn is_done(&self) -> bool {
        matches!(self.end, Some(e) if self.start + self.written > e)
    }

    /// Reserve up to `want` bytes; returns (file offset, allowed bytes).
    pub fn claim(&mut self, want: usize) -> (u64, usize) {
        let offset = self.start + self.claimed;
        let allowed = match self.remaining() {
            Some(r) => (want as u64).min(r) as usize,
            None => want,
        };
        self.claimed += allowed as u64;
        (offset, allowed)
    }
}

pub type ChunkRef = Arc<Mutex<ChunkShared>>;

pub struct ChunkTable {
    chunks: Vec<ChunkRef>,
}

impl ChunkTable {
    pub fn from_records(recs: &[ChunkRecord]) -> Self {
        let chunks = recs
            .iter()
            .map(|r| {
                Arc::new(Mutex::new(ChunkShared {
                    idx: r.idx, start: r.start, end: r.end, claimed: r.written, written: r.written,
                }))
            })
            .collect();
        Self { chunks }
    }

    pub fn refs(&self) -> Vec<ChunkRef> {
        self.chunks.clone()
    }

    pub fn pending(&self) -> Vec<ChunkRef> {
        self.chunks.iter().filter(|c| !c.lock().unwrap().is_done()).cloned().collect()
    }

    pub fn split_largest(&mut self) -> Option<ChunkRef> {
        let next_idx = self.chunks.iter().map(|c| c.lock().unwrap().idx).max().map_or(0, |m| m + 1);
        let victim = self
            .chunks
            .iter()
            .filter_map(|c| c.lock().unwrap().remaining().map(|r| (r, c.clone())))
            .filter(|(r, _)| *r > MIN_SPLIT_REMAINING)
            .max_by_key(|(r, _)| *r)?
            .1;
        let mut v = victim.lock().unwrap();
        let remaining = v.remaining()?; // re-check under the lock
        if remaining <= MIN_SPLIT_REMAINING {
            return None;
        }
        let cur = v.start + v.claimed;
        let mid = cur + remaining / 2;
        let new = ChunkShared { idx: next_idx, start: mid, end: v.end, claimed: 0, written: 0 };
        v.end = Some(mid - 1);
        drop(v);
        let r = Arc::new(Mutex::new(new));
        self.chunks.push(r.clone());
        Some(r)
    }

    pub fn snapshot(&self) -> Vec<ChunkRecord> {
        let mut v: Vec<ChunkRecord> = self
            .chunks
            .iter()
            .map(|c| {
                let c = c.lock().unwrap();
                ChunkRecord { idx: c.idx, start: c.start, end: c.end, written: c.written }
            })
            .collect();
        v.sort_by_key(|c| c.start);
        v
    }

    pub fn progress(&self) -> Vec<(u32, u64)> {
        self.chunks.iter().map(|c| { let c = c.lock().unwrap(); (c.idx, c.written) }).collect()
    }

    pub fn total_written(&self) -> u64 {
        self.chunks.iter().map(|c| c.lock().unwrap().written).sum()
    }
}
```

Note on `split_largest`: the victim is chosen while each chunk's lock is held only briefly. Its remaining bytes are then **re-checked under its own lock** before cutting, because a worker may have claimed more in the meantime. The cut happens at `mid`, which is at least `remaining/2` bytes past the claimed position, so no worker can have claimed past it.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p odm-engine --lib chunks`
Expected: 10 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/odm-engine
git commit -m "feat(engine): chunk planning, claiming and safe dynamic splitting"
```

---

### Task 8: `odm-testserver`: HTTP server with fault injection

**Files:**
- Create: `crates/odm-testserver/Cargo.toml`, `crates/odm-testserver/src/lib.rs`
- Modify: `crates/odm-engine/Cargo.toml` (uncomment `odm-testserver.workspace = true`)

**Interfaces:**
- Produces:
  - `pub fn body(len: u64) -> Vec<u8>`: deterministic content, byte `i` is `(i % 251) as u8`.
  - `pub struct Faults` (Clone, Default, all `pub`):
    - `no_ranges: bool` (ignore Range, always 200 with the full body)
    - `no_length: bool` (omit Content-Length, chunked)
    - `drop_after: Option<u64>` (close the connection after N body bytes on the first `drop_times` requests)
    - `drop_times: u32`
    - `fail_status: Option<u16>` and `fail_times: u32` (first N requests get this status, with `Retry-After: 1` when 429)
    - `etag_changes_after: Option<u32>` (the ETag becomes `"v2"` after N requests)
    - `forbid_after: Option<u32>` (403 once more than N requests have been made)
    - `wrong_range_start: bool` (206 with the Content-Range start shifted by +1)
    - `bytes_per_sec: Option<u64>` (throttle each response)
    - `filename: Option<String>` (send `Content-Disposition`)
  - `pub struct TestServer { pub base: String, pub requests: Arc<AtomicU32> }`
    - `TestServer::start(len: u64, faults: Faults) -> TestServer` (async). It serves:
      - `GET {base}/file`
      - `GET {base}/redirect` (302 to `/file`)
      - `GET {base}/empty` (a 0-byte file)
    - `url(&self, path: &str) -> String`
    - `max_concurrent(&self) -> u32` (highest number of simultaneous in-flight `/file` responses)
  - The ETag is `"v1"` (or `"v2"` after the change). `If-Range` is honoured: if the given value doesn't equal the current ETag, the server sends 200 with the full body.

- [ ] **Step 1: Manifest**

```toml
[package]
name = "odm-testserver"
edition.workspace = true
version.workspace = true
license.workspace = true
publish = false

[dependencies]
axum.workspace = true
tokio.workspace = true
bytes.workspace = true
futures-util.workspace = true
```

- [ ] **Step 2: Implement `src/lib.rs`**

```rust
//! Fault-injecting HTTP server for engine tests. Not shipped.
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, Response, StatusCode},
    routing::get,
    Router,
};
use bytes::Bytes;
use futures_util::stream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub fn body(len: u64) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

#[derive(Clone, Default)]
pub struct Faults {
    pub no_ranges: bool,
    pub no_length: bool,
    pub drop_after: Option<u64>,
    pub drop_times: u32,
    pub fail_status: Option<u16>,
    pub fail_times: u32,
    pub etag_changes_after: Option<u32>,
    pub forbid_after: Option<u32>,
    pub wrong_range_start: bool,
    pub bytes_per_sec: Option<u64>,
    pub filename: Option<String>,
}

struct Inner {
    data: Arc<Vec<u8>>,
    faults: Faults,
    requests: Arc<AtomicU32>,
    in_flight: AtomicU32,
    max_in_flight: AtomicU32,
}

pub struct TestServer {
    pub base: String,
    pub requests: Arc<AtomicU32>,
    inner: Arc<Inner>,
}

impl TestServer {
    pub async fn start(len: u64, faults: Faults) -> TestServer {
        let requests = Arc::new(AtomicU32::new(0));
        let inner = Arc::new(Inner {
            data: Arc::new(body(len)),
            faults,
            requests: requests.clone(),
            in_flight: AtomicU32::new(0),
            max_in_flight: AtomicU32::new(0),
        });
        let app = Router::new()
            .route("/file", get(file))
            .route("/empty", get(empty))
            .route("/redirect", get(|| async {
                Response::builder().status(302).header(header::LOCATION, "/file").body(Body::empty()).unwrap()
            }))
            .with_state(inner.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        TestServer { base: format!("http://{addr}"), requests, inner }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    pub fn max_concurrent(&self) -> u32 {
        self.inner.max_in_flight.load(Ordering::SeqCst)
    }
}

async fn empty() -> Response<Body> {
    Response::builder()
        .status(200)
        .header(header::CONTENT_LENGTH, "0")
        .header(header::ACCEPT_RANGES, "bytes")
        .body(Body::empty())
        .unwrap()
}

fn parse_range(h: &HeaderMap, len: u64) -> Option<(u64, u64)> {
    if len == 0 {
        return None;
    }
    let v = h.get(header::RANGE)?.to_str().ok()?;
    let spec = v.strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    let start: u64 = a.parse().ok()?;
    let end: u64 = if b.is_empty() { len - 1 } else { b.parse::<u64>().ok()?.min(len - 1) };
    (start <= end && start < len).then_some((start, end))
}

async fn file(State(s): State<Arc<Inner>>, h: HeaderMap) -> Response<Body> {
    let n = s.requests.fetch_add(1, Ordering::SeqCst) + 1;
    let f = &s.faults;
    let len = s.data.len() as u64;

    if let Some(code) = f.fail_status {
        if n <= f.fail_times {
            let mut b = Response::builder().status(code);
            if code == 429 {
                b = b.header(header::RETRY_AFTER, "1");
            }
            return b.body(Body::empty()).unwrap();
        }
    }
    if matches!(f.forbid_after, Some(k) if n > k) {
        return Response::builder().status(403).body(Body::empty()).unwrap();
    }

    let etag = match f.etag_changes_after {
        Some(k) if n > k => "\"v2\"",
        _ => "\"v1\"",
    };
    let if_range_ok = h
        .get(header::IF_RANGE)
        .and_then(|v| v.to_str().ok())
        .map_or(true, |v| v == etag);

    let range = if f.no_ranges || !if_range_ok { None } else { parse_range(&h, len) };
    if !f.no_ranges && h.contains_key(header::RANGE) && range.is_none() && if_range_ok {
        return Response::builder()
            .status(416)
            .header(header::CONTENT_RANGE, format!("bytes */{len}"))
            .body(Body::empty())
            .unwrap();
    }

    let (status, start, end) = match range {
        Some((a, b)) => (StatusCode::PARTIAL_CONTENT, a, b),
        None => (StatusCode::OK, 0, len.saturating_sub(1)),
    };
    let mut slice = if len == 0 { Vec::new() } else { s.data[start as usize..=end as usize].to_vec() };
    if let Some(k) = f.drop_after {
        if n <= f.drop_times {
            slice.truncate(k as usize);
        }
    }
    let dropping = matches!(f.drop_after, Some(_) if n <= f.drop_times);

    let mut b = Response::builder()
        .status(status)
        .header(header::ETAG, etag)
        .header(header::CONTENT_TYPE, "application/octet-stream");
    if !f.no_ranges {
        b = b.header(header::ACCEPT_RANGES, "bytes");
    }
    if status == StatusCode::PARTIAL_CONTENT {
        let shown = if f.wrong_range_start { start + 1 } else { start };
        b = b.header(header::CONTENT_RANGE, format!("bytes {shown}-{end}/{len}"));
    }
    if !f.no_length {
        // When dropping, advertise the full length so the client sees a short body.
        b = b.header(header::CONTENT_LENGTH, (end - start + 1).to_string());
    }
    if let Some(name) = &f.filename {
        b = b.header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\""));
    }

    let rate = f.bytes_per_sec;
    let inner = s.clone();
    inner.in_flight.fetch_add(1, Ordering::SeqCst);
    let cur = inner.in_flight.load(Ordering::SeqCst);
    inner.max_in_flight.fetch_max(cur, Ordering::SeqCst);

    let piece = 16 * 1024;
    let chunks: Vec<Bytes> = slice.chunks(piece).map(Bytes::copy_from_slice).collect();
    let total = chunks.len();
    let st = stream::unfold((0usize, chunks, inner), move |(i, chunks, inner)| async move {
        if i == usize::MAX {
            return None; // error already emitted
        }
        if i >= total {
            inner.in_flight.fetch_sub(1, Ordering::SeqCst);
            if dropping {
                return Some((Err(std::io::Error::other("dropped")), (usize::MAX, chunks, inner)));
            }
            return None;
        }
        if let Some(r) = rate {
            tokio::time::sleep(Duration::from_secs_f64(chunks[i].len() as f64 / r as f64)).await;
        }
        let c = chunks[i].clone();
        Some((Ok::<_, std::io::Error>(c), (i + 1, chunks, inner)))
    });
    b.body(Body::from_stream(st)).unwrap()
}
```

Note: `usize::MAX` is a sentinel meaning "the error has already been emitted, end the stream". It must be checked *before* the `i >= total` branch.

If the client disconnects early (a cancelled worker), the stream is dropped without reaching the end, so `in_flight` isn't decremented. `max_concurrent` is only used as a *peak* measure in the concurrency tests, so this is acceptable.

- [ ] **Step 3: Enable it for the engine and write a smoke test**

In `crates/odm-engine/Cargo.toml`, uncomment `odm-testserver.workspace = true`.

Create `crates/odm-engine/tests/testserver_smoke.rs`:
```rust
use odm_testserver::{body, Faults, TestServer};

#[tokio::test]
async fn serves_ranges_and_faults() {
    let s = TestServer::start(1000, Faults::default()).await;
    let c = reqwest::Client::new();
    let r = c.get(s.url("/file")).header("Range", "bytes=10-19").send().await.unwrap();
    assert_eq!(r.status(), 206);
    assert_eq!(r.headers()["content-range"], "bytes 10-19/1000");
    assert_eq!(r.bytes().await.unwrap().as_ref(), &body(1000)[10..20]);

    let r = c.get(s.url("/file")).header("Range", "bytes=5000-").send().await.unwrap();
    assert_eq!(r.status(), 416);

    let s = TestServer::start(1000, Faults { no_ranges: true, ..Default::default() }).await;
    let r = c.get(s.url("/file")).header("Range", "bytes=10-19").send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.bytes().await.unwrap().len(), 1000);

    let s = TestServer::start(100_000, Faults { drop_after: Some(1000), drop_times: 1, ..Default::default() }).await;
    let r = c.get(s.url("/file")).send().await.unwrap();
    assert!(r.bytes().await.is_err(), "first request must be cut short");
    let r = c.get(s.url("/file")).send().await.unwrap();
    assert_eq!(r.bytes().await.unwrap().len(), 100_000);
}
```

- [ ] **Step 4: Run it**

Run: `cargo test -p odm-engine --test testserver_smoke`
Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/odm-testserver crates/odm-engine
git commit -m "test: fault-injecting HTTP test server"
```

---

### Task 9: `odm-engine`: probe

**Files:**
- Create: `crates/odm-engine/src/probe.rs`
- Modify: `crates/odm-engine/src/lib.rs` (add `pub mod probe; pub use probe::{probe, ProbeResult};`)
- Test: `crates/odm-engine/tests/probe.rs`

**Interfaces:**
- Consumes: `retry::{classify_status, http_err}`, `odm_core::filename::from_content_disposition`
- Produces:
  - `pub struct ProbeResult { pub final_url: String, pub size: Option<u64>, pub supports_range: bool, pub etag: Option<String>, pub last_modified: Option<String>, pub mime: Option<String>, pub filename: Option<String> }`. `filename` comes only from Content-Disposition.
  - `ProbeResult::validator(&self) -> Option<String>`: the strong ETag if there is one (a weak `W/` tag doesn't count), otherwise Last-Modified.
  - `pub async fn probe(client: &reqwest::Client, url: &str, headers: &reqwest::header::HeaderMap) -> Result<ProbeResult, OdmError>`
  - `pub fn parse_content_range_total(v: &str) -> Option<u64>` (`"bytes 0-0/1234"` gives 1234; `"bytes */0"` gives 0; `"bytes 0-0/*"` gives None)

- [ ] **Step 1: Write the failing tests**

`crates/odm-engine/tests/probe.rs`:
```rust
use odm_core::ErrorKind;
use odm_engine::probe::{parse_content_range_total, probe};
use odm_testserver::{Faults, TestServer};
use reqwest::header::HeaderMap;

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

#[test]
fn content_range_parsing() {
    assert_eq!(parse_content_range_total("bytes 0-0/1234"), Some(1234));
    assert_eq!(parse_content_range_total("bytes */0"), Some(0));
    assert_eq!(parse_content_range_total("bytes 0-0/*"), None);
    assert_eq!(parse_content_range_total("garbage"), None);
}

#[tokio::test]
async fn probe_range_server() {
    let s = TestServer::start(5_000_000, Faults::default()).await;
    let p = probe(&client(), &s.url("/file"), &HeaderMap::new()).await.unwrap();
    assert_eq!(p.size, Some(5_000_000));
    assert!(p.supports_range);
    assert_eq!(p.etag.as_deref(), Some("\"v1\""));
    assert_eq!(p.validator().as_deref(), Some("\"v1\""));
    assert_eq!(p.mime.as_deref(), Some("application/octet-stream"));
}

#[tokio::test]
async fn probe_no_range_server() {
    let s = TestServer::start(1234, Faults { no_ranges: true, ..Default::default() }).await;
    let p = probe(&client(), &s.url("/file"), &HeaderMap::new()).await.unwrap();
    assert_eq!((p.size, p.supports_range), (Some(1234), false));
}

#[tokio::test]
async fn probe_unknown_size() {
    let s = TestServer::start(1234, Faults { no_ranges: true, no_length: true, ..Default::default() }).await;
    let p = probe(&client(), &s.url("/file"), &HeaderMap::new()).await.unwrap();
    assert_eq!((p.size, p.supports_range), (None, false));
}

#[tokio::test]
async fn probe_zero_length_416() {
    let s = TestServer::start(0, Faults::default()).await;
    let p = probe(&client(), &s.url("/file"), &HeaderMap::new()).await.unwrap();
    assert_eq!(p.size, Some(0));
}

#[tokio::test]
async fn probe_zero_length_200() {
    let s = TestServer::start(0, Faults::default()).await;
    let p = probe(&client(), &s.url("/empty"), &HeaderMap::new()).await.unwrap();
    assert_eq!(p.size, Some(0));
}

#[tokio::test]
async fn probe_follows_redirect_and_reads_disposition() {
    let s = TestServer::start(100, Faults { filename: Some("report.pdf".into()), ..Default::default() }).await;
    let p = probe(&client(), &s.url("/redirect"), &HeaderMap::new()).await.unwrap();
    assert!(p.final_url.ends_with("/file"), "{}", p.final_url);
    assert_eq!(p.filename.as_deref(), Some("report.pdf"));
}

#[tokio::test]
async fn probe_http_error_is_classified() {
    let s = TestServer::start(100, Faults { fail_status: Some(404), fail_times: 99, ..Default::default() }).await;
    let e = probe(&client(), &s.url("/file"), &HeaderMap::new()).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::Http(404));
    let s = TestServer::start(100, Faults { fail_status: Some(503), fail_times: 99, ..Default::default() }).await;
    let e = probe(&client(), &s.url("/file"), &HeaderMap::new()).await.unwrap_err();
    assert!(e.kind.is_transient());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-engine --test probe`
Expected: compile error, "could not find `probe`".

- [ ] **Step 3: Implement `src/probe.rs`**

```rust
use odm_core::{filename, OdmError};
use reqwest::header::{self, HeaderMap};

use crate::retry::{classify_status, http_err};

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeResult {
    pub final_url: String,
    pub size: Option<u64>,
    pub supports_range: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub mime: Option<String>,
    pub filename: Option<String>,
}

impl ProbeResult {
    /// Value for If-Range: strong ETag preferred, else Last-Modified.
    pub fn validator(&self) -> Option<String> {
        self.etag
            .clone()
            .filter(|e| !e.starts_with("W/"))
            .or_else(|| self.last_modified.clone())
    }
}

pub fn parse_content_range_total(v: &str) -> Option<u64> {
    let rest = v.trim().strip_prefix("bytes ")?;
    let (_, total) = rest.split_once('/')?;
    total.trim().parse().ok()
}

fn header_str(h: &HeaderMap, name: header::HeaderName) -> Option<String> {
    h.get(name).and_then(|v| v.to_str().ok()).map(str::to_string)
}

pub async fn probe(client: &reqwest::Client, url: &str, headers: &HeaderMap) -> Result<ProbeResult, OdmError> {
    let resp = client
        .get(url)
        .headers(headers.clone())
        .header(header::RANGE, "bytes=0-0")
        .send()
        .await
        .map_err(http_err)?;
    let status = resp.status().as_u16();
    let h = resp.headers().clone();
    let final_url = resp.url().to_string();
    drop(resp); // never read the body of a probe

    let (size, supports_range) = match status {
        206 => (h.get(header::CONTENT_RANGE).and_then(|v| v.to_str().ok()).and_then(parse_content_range_total), true),
        416 => (
            Some(h.get(header::CONTENT_RANGE).and_then(|v| v.to_str().ok()).and_then(parse_content_range_total).unwrap_or(0)),
            true,
        ),
        200 => (h.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()), false),
        s => return Err(OdmError::new(classify_status(s, false), format!("probe returned HTTP {s}"))),
    };

    Ok(ProbeResult {
        final_url,
        size,
        supports_range,
        etag: header_str(&h, header::ETAG),
        last_modified: header_str(&h, header::LAST_MODIFIED),
        mime: header_str(&h, header::CONTENT_TYPE).map(|m| m.split(';').next().unwrap_or("").trim().to_string()),
        filename: header_str(&h, header::CONTENT_DISPOSITION).and_then(|v| filename::from_content_disposition(&v)),
    })
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p odm-engine --test probe`
Expected: 8 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/odm-engine
git commit -m "feat(engine): GET range probe with size, range and validator detection"
```

---

### Task 10: `odm-engine`: chunk worker

**Files:**
- Create: `crates/odm-engine/src/worker.rs`
- Modify: `crates/odm-engine/src/lib.rs` (add `pub mod worker;`)
- Test: `crates/odm-engine/tests/worker.rs`

**Interfaces:**
- Consumes: `chunks::ChunkRef`, `SpeedLimiter`, `retry::{classify_status, http_err, io_err, parse_retry_after}`
- Produces:
  - `pub struct ChunkCtx { pub client: reqwest::Client, pub url: String, pub headers: HeaderMap, pub validator: Option<String>, pub file: Arc<std::fs::File>, pub global_limit: Arc<SpeedLimiter>, pub local_limit: Option<Arc<SpeedLimiter>>, pub cancel: CancellationToken, pub require_partial: bool, pub session_bytes: Arc<AtomicU64> }`. It is Clone.
  - `pub struct ChunkFailure { pub error: OdmError, pub retry_after: Option<Duration> }`
  - `pub async fn run_chunk(ctx: &ChunkCtx, chunk: &ChunkRef) -> Result<(), ChunkFailure>`
    - On start it resets `claimed = written`, so it resumes exactly at the flushed position.
    - It sends `Range` when `require_partial` is set or the offset is greater than 0, and adds `If-Range` alongside Range when a validator is present.
    - `206` with a Content-Range start that doesn't match the offset gives `RangeIgnored`.
    - `200` when a Range was sent gives `FileChanged` if a validator was sent, otherwise `RangeIgnored`.
    - Other statuses map through `classify_status(s, true)`, with `retry_after` taken from the header.
    - It never writes past the chunk's (possibly moving) end.
    - A stream that ends early gives `Connection` (transient).
    - For an open-ended chunk, the end of the stream means success, and `end` is set to the last written byte.
    - Cancellation gives `ErrorKind::Cancelled`.
  - `pub fn write_at(file: &std::fs::File, offset: u64, buf: &[u8]) -> std::io::Result<()>`

- [ ] **Step 1: Write the failing tests**

`crates/odm-engine/tests/worker.rs`:
```rust
use odm_core::ErrorKind;
use odm_engine::chunks::ChunkTable;
use odm_engine::worker::{run_chunk, ChunkCtx};
use odm_engine::SpeedLimiter;
use odm_store::ChunkRecord;
use odm_testserver::{body, Faults, TestServer};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

fn ctx(url: String, file: std::fs::File, require_partial: bool, validator: Option<&str>) -> ChunkCtx {
    ChunkCtx {
        client: reqwest::Client::new(),
        url,
        headers: Default::default(),
        validator: validator.map(str::to_string),
        file: Arc::new(file),
        global_limit: Arc::new(SpeedLimiter::new(0)),
        local_limit: None,
        cancel: CancellationToken::new(),
        require_partial,
        session_bytes: Arc::new(AtomicU64::new(0)),
    }
}

fn tmpfile(len: u64) -> (tempfile::TempDir, std::path::PathBuf, std::fs::File) {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("out.bin");
    let f = std::fs::OpenOptions::new().create(true).read(true).write(true).open(&p).unwrap();
    f.set_len(len).unwrap();
    (d, p, f)
}

fn table(start: u64, end: Option<u64>, written: u64) -> ChunkTable {
    ChunkTable::from_records(&[ChunkRecord { idx: 0, start, end, written }])
}

#[tokio::test]
async fn downloads_a_range_exactly() {
    let s = TestServer::start(300_000, Faults::default()).await;
    let (_d, p, f) = tmpfile(300_000);
    let t = table(100_000, Some(199_999), 0);
    run_chunk(&ctx(s.url("/file"), f, true, Some("\"v1\"")), &t.refs()[0]).await.unwrap();
    let got = std::fs::read(&p).unwrap();
    assert_eq!(&got[100_000..200_000], &body(300_000)[100_000..200_000]);
    assert!(got[..100_000].iter().all(|&b| b == 0));
    assert_eq!(t.total_written(), 100_000);
}

#[tokio::test]
async fn stops_at_end_even_if_server_sends_more() {
    let s = TestServer::start(50_000, Faults { no_ranges: true, ..Default::default() }).await;
    let (_d, p, f) = tmpfile(1_000);
    let t = table(0, Some(999), 0);
    run_chunk(&ctx(s.url("/file"), f, false, None), &t.refs()[0]).await.unwrap();
    let got = std::fs::read(&p).unwrap();
    assert_eq!(got.len(), 1_000);
    assert_eq!(got, &body(50_000)[..1_000]);
}

#[tokio::test]
async fn range_ignored_when_partial_required() {
    let s = TestServer::start(50_000, Faults { no_ranges: true, ..Default::default() }).await;
    let (_d, _p, f) = tmpfile(50_000);
    let t = table(10_000, Some(19_999), 0);
    let e = run_chunk(&ctx(s.url("/file"), f, true, None), &t.refs()[0]).await.unwrap_err();
    assert_eq!(e.error.kind, ErrorKind::RangeIgnored);
    assert_eq!(t.total_written(), 0);
}

#[tokio::test]
async fn wrong_content_range_start_is_range_ignored() {
    let s = TestServer::start(50_000, Faults { wrong_range_start: true, ..Default::default() }).await;
    let (_d, _p, f) = tmpfile(50_000);
    let t = table(10_000, Some(19_999), 0);
    let e = run_chunk(&ctx(s.url("/file"), f, true, None), &t.refs()[0]).await.unwrap_err();
    assert_eq!(e.error.kind, ErrorKind::RangeIgnored);
}

#[tokio::test]
async fn dropped_connection_then_resume_completes() {
    let s = TestServer::start(200_000, Faults { drop_after: Some(50_000), drop_times: 1, ..Default::default() }).await;
    let (_d, p, f) = tmpfile(200_000);
    let t = table(0, Some(199_999), 0);
    let c = ctx(s.url("/file"), f, true, Some("\"v1\""));
    let e = run_chunk(&c, &t.refs()[0]).await.unwrap_err();
    assert!(e.error.kind.is_transient(), "{:?}", e.error);
    let partial = t.total_written();
    assert!(partial > 0 && partial <= 50_000, "{partial}");
    run_chunk(&c, &t.refs()[0]).await.unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), body(200_000));
}

#[tokio::test]
async fn changed_file_is_detected_via_if_range() {
    let s = TestServer::start(50_000, Faults { etag_changes_after: Some(0), ..Default::default() }).await;
    let (_d, _p, f) = tmpfile(50_000);
    let t = table(0, Some(49_999), 10_000);
    let e = run_chunk(&ctx(s.url("/file"), f, true, Some("\"v1\"")), &t.refs()[0]).await.unwrap_err();
    assert_eq!(e.error.kind, ErrorKind::FileChanged);
}

#[tokio::test]
async fn cancel_stops_promptly() {
    let s = TestServer::start(5_000_000, Faults { bytes_per_sec: Some(200_000), ..Default::default() }).await;
    let (_d, _p, f) = tmpfile(5_000_000);
    let t = table(0, Some(4_999_999), 0);
    let c = ctx(s.url("/file"), f, true, None);
    let tok = c.cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        tok.cancel();
    });
    let started = std::time::Instant::now();
    let e = run_chunk(&c, &t.refs()[0]).await.unwrap_err();
    assert_eq!(e.error.kind, ErrorKind::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(t.total_written() < 5_000_000);
}

#[tokio::test]
async fn statuses_map_to_kinds_with_retry_after() {
    let s = TestServer::start(1000, Faults { fail_status: Some(429), fail_times: 9, ..Default::default() }).await;
    let (_d, _p, f) = tmpfile(1000);
    let t = table(0, Some(999), 0);
    let e = run_chunk(&ctx(s.url("/file"), f, true, None), &t.refs()[0]).await.unwrap_err();
    assert_eq!(e.error.kind, ErrorKind::HttpTransient(429));
    assert_eq!(e.retry_after, Some(Duration::from_secs(1)));

    let s = TestServer::start(1000, Faults { forbid_after: Some(0), ..Default::default() }).await;
    let (_d, _p, f) = tmpfile(1000);
    let t = table(0, Some(999), 0);
    let e = run_chunk(&ctx(s.url("/file"), f, true, None), &t.refs()[0]).await.unwrap_err();
    assert_eq!(e.error.kind, ErrorKind::LinkExpired);
}

#[tokio::test]
async fn open_ended_stream_completes_and_sets_end() {
    let s = TestServer::start(70_000, Faults { no_ranges: true, no_length: true, ..Default::default() }).await;
    let (_d, p, f) = tmpfile(0);
    let t = table(0, None, 0);
    run_chunk(&ctx(s.url("/file"), f, false, None), &t.refs()[0]).await.unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), body(70_000));
    assert_eq!(t.snapshot()[0].end, Some(69_999));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-engine --test worker`
Expected: compile error, "could not find `worker`".

- [ ] **Step 3: Implement `src/worker.rs`**

```rust
use futures_util::StreamExt;
use odm_core::{ErrorKind, OdmError};
use reqwest::header::{self, HeaderMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::chunks::ChunkRef;
use crate::retry::{classify_status, http_err, io_err, parse_retry_after};
use crate::SpeedLimiter;

#[derive(Clone)]
pub struct ChunkCtx {
    pub client: reqwest::Client,
    pub url: String,
    pub headers: HeaderMap,
    pub validator: Option<String>,
    pub file: Arc<std::fs::File>,
    pub global_limit: Arc<SpeedLimiter>,
    pub local_limit: Option<Arc<SpeedLimiter>>,
    pub cancel: CancellationToken,
    pub require_partial: bool,
    pub session_bytes: Arc<AtomicU64>,
}

#[derive(Debug, Clone)]
pub struct ChunkFailure {
    pub error: OdmError,
    pub retry_after: Option<Duration>,
}

impl From<OdmError> for ChunkFailure {
    fn from(error: OdmError) -> Self {
        Self { error, retry_after: None }
    }
}

fn fail(kind: ErrorKind, msg: impl Into<String>) -> ChunkFailure {
    OdmError::new(kind, msg).into()
}

pub fn write_at(file: &std::fs::File, mut offset: u64, mut buf: &[u8]) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            let n = file.seek_write(buf, offset)?;
            if n == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::WriteZero));
            }
            buf = &buf[n..];
            offset += n as u64;
        }
        Ok(())
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.write_all_at(buf, offset)
    }
}

/// Parses the start offset of "bytes a-b/total".
fn content_range_start(h: &HeaderMap) -> Option<u64> {
    let v = h.get(header::CONTENT_RANGE)?.to_str().ok()?;
    let rest = v.trim().strip_prefix("bytes ")?;
    rest.split('-').next()?.trim().parse().ok()
}

pub async fn run_chunk(ctx: &ChunkCtx, chunk: &ChunkRef) -> Result<(), ChunkFailure> {
    let (offset, end) = {
        let mut c = chunk.lock().unwrap();
        c.claimed = c.written; // discard un-flushed reservations from a failed attempt
        if c.is_done() {
            return Ok(());
        }
        (c.start + c.written, c.end)
    };

    let send_range = ctx.require_partial || offset > 0;
    let mut req = ctx.client.get(&ctx.url).headers(ctx.headers.clone());
    if send_range {
        let r = match end {
            Some(e) => format!("bytes={offset}-{e}"),
            None => format!("bytes={offset}-"),
        };
        req = req.header(header::RANGE, r);
        if let Some(v) = &ctx.validator {
            req = req.header(header::IF_RANGE, v);
        }
    }

    let resp = tokio::select! {
        _ = ctx.cancel.cancelled() => return Err(fail(ErrorKind::Cancelled, "cancelled")),
        r = req.send() => r.map_err(|e| ChunkFailure::from(http_err(e)))?,
    };

    let status = resp.status().as_u16();
    match status {
        206 => {
            if content_range_start(resp.headers()) != Some(offset) {
                return Err(fail(ErrorKind::RangeIgnored, "206 with unexpected Content-Range"));
            }
        }
        200 if send_range && ctx.validator.is_some() => {
            return Err(fail(ErrorKind::FileChanged, "server file changed (If-Range mismatch)"));
        }
        200 if send_range => return Err(fail(ErrorKind::RangeIgnored, "server ignored Range")),
        200 => {}
        s => {
            let retry_after = resp
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            return Err(ChunkFailure {
                error: OdmError::new(classify_status(s, true), format!("HTTP {s}")),
                retry_after,
            });
        }
    }

    let mut stream = resp.bytes_stream();
    loop {
        let next = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(fail(ErrorKind::Cancelled, "cancelled")),
            n = stream.next() => n,
        };
        let data = match next {
            None => break,
            Some(Err(e)) => return Err(http_err(e).into()),
            Some(Ok(d)) => d,
        };

        tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(fail(ErrorKind::Cancelled, "cancelled")),
            _ = async {
                ctx.global_limit.acquire(data.len()).await;
                if let Some(l) = &ctx.local_limit { l.acquire(data.len()).await; }
            } => {}
        }

        let (off, allowed) = chunk.lock().unwrap().claim(data.len());
        if allowed > 0 {
            write_at(&ctx.file, off, &data[..allowed]).map_err(|e| ChunkFailure::from(io_err(e)))?;
            chunk.lock().unwrap().written += allowed as u64;
            ctx.session_bytes.fetch_add(allowed as u64, Ordering::Relaxed);
        }
        if allowed < data.len() {
            return Ok(()); // reached this chunk's end (possibly moved by a split)
        }
    }

    let mut c = chunk.lock().unwrap();
    match c.end {
        None => {
            if c.written > 0 {
                c.end = Some(c.start + c.written - 1);
            }
            Ok(())
        }
        Some(_) if c.is_done() => Ok(()),
        Some(_) => Err(fail(ErrorKind::Connection, "server closed the connection early")),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p odm-engine --test worker`
Expected: 9 passed.

If `dropped_connection_then_resume_completes` reports `partial == 0`: the test server sends 16 KiB pieces, so at least 3 pieces arrive before the drop. Check that `written` is incremented after every write, not only at the end of the stream.

- [ ] **Step 5: Commit**

```bash
git add crates/odm-engine
git commit -m "feat(engine): range chunk worker with bounded writes and validation"
```

---

### Task 11: `odm-engine`: speed meter, events and config

**Files:**
- Create: `crates/odm-engine/src/speed.rs`, `crates/odm-engine/src/events.rs`, `crates/odm-engine/src/config.rs`
- Modify: `crates/odm-engine/src/lib.rs` (add `pub mod speed; pub mod events; pub mod config;` and re-export `EngineConfig, EngineEvent, ProgressSnapshot, ChunkView`)

**Interfaces:**
- Produces:
  - `SpeedMeter::new(window: Duration)`, `.record(&mut self, now: Instant, total_session_bytes: u64)`, `.bps(&self) -> u64` (0 with fewer than 2 samples)
  - `#[derive(Debug, Clone, Serialize)] pub struct ChunkView { pub start: u64, pub end: Option<u64>, pub written: u64 }`
  - `#[derive(Debug, Clone, Serialize)] pub struct ProgressSnapshot { pub id: String, pub bytes: u64, pub total: Option<u64>, pub speed_bps: u64, pub eta_s: Option<u64>, pub chunks: Vec<ChunkView> }`
  - `#[derive(Debug, Clone, Serialize)] #[serde(tag = "type", rename_all = "snake_case")] pub enum EngineEvent { Added { id: String }, Progress(ProgressSnapshot), State { id: String, state: DownloadState, error_code: Option<String>, error_message: Option<String> }, Removed { id: String } }`
  - `#[derive(Debug, Clone)] pub struct EngineConfig { pub download_dir: PathBuf, pub category_folders: HashMap<String, String>, pub max_concurrent: usize, pub max_connections: u32, pub max_attempts: u32, pub connect_timeout: Duration, pub read_timeout: Duration, pub speed_limit_bps: u64, pub progress_interval: Duration, pub persist_interval: Duration, pub user_agent: String }`
    - It implements `Default`, using the spec values: 3 concurrent, 8 connections, 5 attempts, 30 s connect, 60 s read, 250 ms progress, 1 s persist, `download_dir = %USERPROFILE%\Downloads`, and category folders from `Category::default_folder`.
    - `EngineConfig::save_dir_for(&self, category: &str) -> PathBuf`
    - `pub fn eta(total: Option<u64>, bytes: u64, bps: u64) -> Option<u64>` (in `speed.rs`)

- [ ] **Step 1: Write the failing tests (inline in `speed.rs` and `config.rs`)**

`speed.rs` tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn moving_average_over_window() {
        let t0 = Instant::now();
        let mut m = SpeedMeter::new(Duration::from_secs(5));
        assert_eq!(m.bps(), 0);
        m.record(t0, 0);
        m.record(t0 + Duration::from_secs(1), 1_000);
        assert_eq!(m.bps(), 1_000);
        m.record(t0 + Duration::from_secs(2), 3_000);
        assert_eq!(m.bps(), 1_500);
        // Samples older than 5 s drop out: window now (4s..8s]
        m.record(t0 + Duration::from_secs(8), 3_000);
        assert_eq!(m.bps(), 0);
    }

    #[test]
    fn eta_math() {
        assert_eq!(eta(Some(10_000), 4_000, 1_000), Some(6));
        assert_eq!(eta(None, 4_000, 1_000), None);
        assert_eq!(eta(Some(10_000), 4_000, 0), None);
    }
}
```

`config.rs` tests:
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn defaults_and_category_dirs() {
        let c = super::EngineConfig::default();
        assert_eq!((c.max_concurrent, c.max_connections, c.max_attempts), (3, 8, 5));
        assert!(c.save_dir_for("video").ends_with("Videos"));
        assert_eq!(c.save_dir_for("general"), c.download_dir);
        assert_eq!(c.save_dir_for("unknown"), c.download_dir);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-engine --lib`
Expected: compile error.

- [ ] **Step 3: Implement**

`src/speed.rs` (above the tests):
```rust
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub struct SpeedMeter {
    window: Duration,
    samples: VecDeque<(Instant, u64)>,
}

impl SpeedMeter {
    pub fn new(window: Duration) -> Self {
        Self { window, samples: VecDeque::new() }
    }

    pub fn record(&mut self, now: Instant, total: u64) {
        self.samples.push_back((now, total));
        while self.samples.len() > 1 && now.duration_since(self.samples[0].0) > self.window {
            self.samples.pop_front();
        }
    }

    pub fn bps(&self) -> u64 {
        let (Some(&(t0, b0)), Some(&(t1, b1))) = (self.samples.front(), self.samples.back()) else {
            return 0;
        };
        let dt = t1.duration_since(t0).as_secs_f64();
        if self.samples.len() < 2 || dt <= 0.0 {
            return 0;
        }
        ((b1.saturating_sub(b0)) as f64 / dt) as u64
    }
}

pub fn eta(total: Option<u64>, bytes: u64, bps: u64) -> Option<u64> {
    let total = total?;
    (bps > 0).then(|| total.saturating_sub(bytes) / bps)
}
```

(In the test, after `record(t0+8s, 3000)` only the sample at t0+8s remains, so there are fewer than 2 samples and `bps` returns 0. That is the intended behaviour: no data in the last 5 s means a speed of 0.)

`src/events.rs`:
```rust
use odm_core::DownloadState;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChunkView {
    pub start: u64,
    pub end: Option<u64>,
    pub written: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProgressSnapshot {
    pub id: String,
    pub bytes: u64,
    pub total: Option<u64>,
    pub speed_bps: u64,
    pub eta_s: Option<u64>,
    pub chunks: Vec<ChunkView>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    Added { id: String },
    Progress(ProgressSnapshot),
    State {
        id: String,
        state: DownloadState,
        error_code: Option<String>,
        error_message: Option<String>,
    },
    Removed { id: String },
}
```

`src/config.rs` (above the tests):
```rust
use odm_core::Category;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub download_dir: PathBuf,
    pub category_folders: HashMap<String, String>,
    pub max_concurrent: usize,
    pub max_connections: u32,
    pub max_attempts: u32,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub speed_limit_bps: u64,
    pub progress_interval: Duration,
    pub persist_interval: Duration,
    pub user_agent: String,
}

impl Default for EngineConfig {
    fn default() -> Self {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            download_dir: home.join("Downloads"),
            category_folders: Category::ALL
                .iter()
                .map(|c| (c.as_str().to_string(), c.default_folder().to_string()))
                .collect(),
            max_concurrent: 3,
            max_connections: 8,
            max_attempts: 5,
            connect_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(60),
            speed_limit_bps: 0,
            progress_interval: Duration::from_millis(250),
            persist_interval: Duration::from_secs(1),
            user_agent: format!("ODM/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

impl EngineConfig {
    pub fn save_dir_for(&self, category: &str) -> PathBuf {
        match self.category_folders.get(category).map(String::as_str) {
            Some(f) if !f.is_empty() => self.download_dir.join(f),
            _ => self.download_dir.clone(),
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p odm-engine --lib`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/odm-engine
git commit -m "feat(engine): speed meter, engine events and config"
```

---

### Task 12: `odm-engine`: download task (probe → plan → transfer → finish)

**Files:**
- Create: `crates/odm-engine/src/download.rs`
- Modify: `crates/odm-engine/src/lib.rs` (add `pub mod download;` and `pub use download::{run_download, DownloadEnv, Outcome};`)
- Modify: `Cargo.toml` (workspace deps: add `md-5 = "0.10"`), and `crates/odm-engine/Cargo.toml` (move `sha2` from dev-dependencies to `[dependencies]` and add `md-5.workspace = true`)
- Test: `crates/odm-engine/tests/download.rs`

**Interfaces:**
- Consumes: everything from Tasks 5–11, plus `odm_store::{Store, DownloadRecord, ChunkRecord, ProbeMeta}` and `odm_core::filename::unique_name`
- Produces:
  - `#[derive(Clone)] pub struct DownloadEnv { pub store: Store, pub client: reqwest::Client, pub limiter: Arc<SpeedLimiter>, pub events: tokio::sync::broadcast::Sender<EngineEvent>, pub config: EngineConfig }`
  - `#[derive(Debug)] pub enum Outcome { Completed { path: PathBuf }, Stopped, Failed(OdmError) }`
  - `pub async fn run_download(env: &DownloadEnv, id: &str, cancel: CancellationToken) -> Outcome`. It sets `probing` and `downloading` itself, but **never** sets the final state; the manager does that from the returned `Outcome`.
  - `pub async fn transition(env: &DownloadEnv, id: &str, state: DownloadState, err: Option<&OdmError>)`: updates the store and emits `EngineEvent::State`.
  - `pub fn part_path(rec: &DownloadRecord) -> PathBuf` (`<save_dir>/<filename>.<id[..8]>.odm-part`)
  - `pub fn build_client(cfg: &EngineConfig) -> reqwest::Client`
  - `pub fn log_url(url: &str) -> &str` (strips the query and fragment)
  - `pub fn header_map(h: &[(String, String)]) -> HeaderMap` (silently skips invalid pairs)
  - Checksum format: `"sha256:<hex>"` or `"md5:<hex>"`, case-insensitive hex.

**Behaviour (from spec §5.2–5.5):**
- **No chunks stored:** probe (retrying transient errors), save the probe result, plan the chunks, and store them. `require_partial = supports_range && size.is_some()`.
- **Chunks already stored (resume):** skip the probe. `require_partial = chunks.len() > 1`.
- **Size 0:** create an empty part file and finish.
- **Workers:** one per pending chunk. When `require_partial`, keep splitting the largest chunk until there are `max_connections` workers, both at the start and after each chunk finishes.
- **Transient chunk errors:** retry the chunk with backoff. The attempt counter resets whenever that chunk made progress. Once a chunk reaches `max_attempts` without progress, the download fails.
- **`RangeIgnored`:** restart once, from zero, as a single connection. **`FileChanged`:** restart once, from zero, with a fresh probe. A second occurrence fails the download.
- **Persisting:** every `persist_interval` and at the end. Take the snapshot, then `sync_data()`, then write to the store (`replace_chunks` if a split happened since the last persist, otherwise `save_progress`).
- **Progress:** an `EngineEvent::Progress` every `progress_interval`.
- **Finish:**
  - Verify the byte count and the checksum.
  - Choose a unique final name and rename the part file to it.
  - If the name changed, call `set_filename`.
  - If the size was unknown, store the real size.

- [ ] **Step 1: Write the failing tests**

`crates/odm-engine/tests/download.rs`:
```rust
use odm_core::ErrorKind;
use odm_engine::download::{build_client, part_path, run_download, DownloadEnv, Outcome};
use odm_engine::{EngineConfig, SpeedLimiter};
use odm_store::{ChunkRecord, DownloadRecord, Store};
use odm_testserver::{body, Faults, TestServer};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const MB: u64 = 1 << 20;

struct Fx {
    env: DownloadEnv,
    dir: tempfile::TempDir,
}

fn fx() -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let mut config = EngineConfig::default();
    config.download_dir = dir.path().to_path_buf();
    config.progress_interval = Duration::from_millis(50);
    config.persist_interval = Duration::from_millis(200);
    config.max_attempts = 5;
    let (events, _) = tokio::sync::broadcast::channel(1024);
    let env = DownloadEnv {
        store: Store::open_in_memory().unwrap(),
        client: build_client(&config),
        limiter: Arc::new(SpeedLimiter::new(0)),
        events,
        config,
    };
    Fx { env, dir }
}

async fn add(f: &Fx, url: &str, conns: u32) -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let mut r = DownloadRecord::new_file(&id, url, "file.bin", f.dir.path().to_str().unwrap());
    r.max_connections = conns;
    f.env.store.insert_download(r).await.unwrap();
    id
}

fn sha(p: &Path) -> Vec<u8> {
    Sha256::digest(std::fs::read(p).unwrap()).to_vec()
}

fn done(o: Outcome) -> PathBuf {
    match o {
        Outcome::Completed { path } => path,
        other => panic!("expected Completed, got {other:?}"),
    }
}

fn no_parts(dir: &Path) -> bool {
    std::fs::read_dir(dir).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".odm-part"))
}

#[tokio::test]
async fn multi_connection_download_matches() {
    let f = fx();
    let s = TestServer::start(8 * MB, Faults { bytes_per_sec: Some(8 * MB), ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(sha(&p), Sha256::digest(body(8 * MB)).to_vec());
    assert!(s.max_concurrent() >= 2, "used {} connections", s.max_concurrent());
    assert!(no_parts(f.dir.path()));
}

#[tokio::test]
async fn no_range_server_uses_single_connection() {
    let f = fx();
    let s = TestServer::start(3 * MB, Faults { no_ranges: true, ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 8).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(3 * MB));
    assert_eq!(f.env.store.get_chunks(&id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn unknown_size_download_completes() {
    let f = fx();
    let s = TestServer::start(500_000, Faults { no_ranges: true, no_length: true, ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 8).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(500_000));
    assert_eq!(f.env.store.get_download(&id).await.unwrap().unwrap().size, Some(500_000));
}

#[tokio::test]
async fn zero_byte_download_completes() {
    let f = fx();
    let s = TestServer::start(0, Faults::default()).await;
    let id = add(&f, &s.url("/file"), 8).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::metadata(p).unwrap().len(), 0);
}

#[tokio::test]
async fn range_ignored_falls_back_to_single_connection() {
    // Probe sees 206 (so ranges look supported) but every range reply has a wrong start.
    let f = fx();
    let s = TestServer::start(4 * MB, Faults { wrong_range_start: true, ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(4 * MB));
}

#[tokio::test]
async fn dropped_connections_are_retried() {
    let f = fx();
    let s = TestServer::start(4 * MB, Faults { drop_after: Some(200_000), drop_times: 4, ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(4 * MB));
}

#[tokio::test]
async fn persistent_transient_error_fails_after_attempts() {
    let mut f = fx();
    f.env.config.max_attempts = 2;
    let s = TestServer::start(1000, Faults { fail_status: Some(503), fail_times: 1000, ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 4).await;
    match run_download(&f.env, &id, CancellationToken::new()).await {
        Outcome::Failed(e) => assert_eq!(e.kind, ErrorKind::HttpTransient(503)),
        o => panic!("{o:?}"),
    }
}

#[tokio::test]
async fn expired_link_fails_without_retry() {
    let f = fx();
    let s = TestServer::start(2 * MB, Faults { forbid_after: Some(1), ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 2).await;
    match run_download(&f.env, &id, CancellationToken::new()).await {
        Outcome::Failed(e) => assert_eq!(e.kind, ErrorKind::LinkExpired),
        o => panic!("{o:?}"),
    }
}

#[tokio::test]
async fn cancel_then_resume_produces_identical_file() {
    let f = fx();
    let s = TestServer::start(6 * MB, Faults { bytes_per_sec: Some(2 * MB), ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 3).await;
    let tok = CancellationToken::new();
    let t2 = tok.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(700)).await;
        t2.cancel();
    });
    assert!(matches!(run_download(&f.env, &id, tok).await, Outcome::Stopped));
    let saved: u64 = f.env.store.get_chunks(&id).await.unwrap().iter().map(|c| c.written).sum();
    assert!(saved > 0 && saved < 6 * MB, "{saved}");

    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(sha(&p), Sha256::digest(body(6 * MB)).to_vec());
}

#[tokio::test]
async fn resume_fills_free_connections_by_splitting() {
    let f = fx();
    let s = TestServer::start(12 * MB, Faults::default()).await;
    let id = add(&f, &s.url("/file"), 2).await;
    let mut rec = f.env.store.get_download(&id).await.unwrap().unwrap();
    rec.size = Some(12 * MB);
    rec.etag = Some("\"v1\"".into());
    f.env.store.delete_download(&id).await.unwrap();
    f.env.store.insert_download(rec.clone()).await.unwrap();
    // First half already on disk and recorded as done; second half untouched.
    let part = part_path(&rec);
    let mut data = body(12 * MB);
    data[6 * MB as usize..].fill(0);
    std::fs::write(&part, &data).unwrap();
    f.env
        .store
        .replace_chunks(&id, vec![
            ChunkRecord { idx: 0, start: 0, end: Some(6 * MB - 1), written: 6 * MB },
            ChunkRecord { idx: 1, start: 6 * MB, end: Some(12 * MB - 1), written: 0 },
        ])
        .await
        .unwrap();

    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(sha(&p), Sha256::digest(body(12 * MB)).to_vec());
    assert!(f.env.store.get_chunks(&id).await.unwrap().len() >= 3, "free connection was not used");
}

#[tokio::test]
async fn changed_server_file_restarts_from_zero() {
    let f = fx();
    let s = TestServer::start(4 * MB, Faults { etag_changes_after: Some(2), ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(4 * MB));
    assert_eq!(f.env.store.get_download(&id).await.unwrap().unwrap().etag.as_deref(), Some("\"v2\""));
}

#[tokio::test]
async fn save_dir_is_a_file_fails_cleanly() {
    let f = fx();
    let s = TestServer::start(1000, Faults::default()).await;
    let blocker = f.dir.path().join("blocker");
    std::fs::write(&blocker, b"x").unwrap();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let r = DownloadRecord::new_file(&id, &s.url("/file"), "a.bin", blocker.to_str().unwrap());
    f.env.store.insert_download(r).await.unwrap();
    match run_download(&f.env, &id, CancellationToken::new()).await {
        Outcome::Failed(e) => assert!(matches!(e.kind, ErrorKind::Io | ErrorKind::PermissionDenied), "{e}"),
        o => panic!("{o:?}"),
    }
}

#[tokio::test]
async fn name_collision_and_disposition() {
    let f = fx();
    let s = TestServer::start(1000, Faults { filename: Some("report.pdf".into()), ..Default::default() }).await;
    std::fs::write(f.dir.path().join("report.pdf"), b"old").unwrap();
    let id = add(&f, &s.url("/file"), 2).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(p.file_name().unwrap(), "report (1).pdf");
    assert_eq!(std::fs::read(f.dir.path().join("report.pdf")).unwrap(), b"old");
    assert_eq!(f.env.store.get_download(&id).await.unwrap().unwrap().filename, "report (1).pdf");
}

#[tokio::test]
async fn checksum_verified() {
    let f = fx();
    let s = TestServer::start(10_000, Faults::default()).await;
    let good = format!("sha256:{}", hex(&Sha256::digest(body(10_000))));
    for (sum, ok) in [(good, true), ("sha256:00".to_string(), false)] {
        let id = add(&f, &s.url("/file"), 1).await;
        f.env.store.call({
            let id = id.clone();
            move |c| c.execute("UPDATE downloads SET checksum=?2 WHERE id=?1", [&id, &sum]).map(|_| ())
        }).await.unwrap();
        match (run_download(&f.env, &id, CancellationToken::new()).await, ok) {
            (Outcome::Completed { .. }, true) => {}
            (Outcome::Failed(e), false) => assert_eq!(e.kind, ErrorKind::ChecksumMismatch),
            (o, _) => panic!("{o:?}"),
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
```

(`uuid` and `sha2` are normal dependencies of `odm-engine`, so integration tests can use them directly.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-engine --test download`
Expected: compile error, "could not find `download`".

- [ ] **Step 3: Implement `src/download.rs`**

```rust
use md5::Md5;
use odm_core::{filename, DownloadState, ErrorKind, OdmError};
use odm_store::{ChunkRecord, DownloadRecord, ProbeMeta, Store, StoreError};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::chunks::{plan_chunks, ChunkRef, ChunkTable};
use crate::config::EngineConfig;
use crate::events::{ChunkView, EngineEvent, ProgressSnapshot};
use crate::probe::{probe, ProbeResult};
use crate::retry::{backoff_delay, io_err};
use crate::speed::{eta, SpeedMeter};
use crate::worker::{run_chunk, ChunkCtx, ChunkFailure};
use crate::SpeedLimiter;

#[derive(Clone)]
pub struct DownloadEnv {
    pub store: Store,
    pub client: reqwest::Client,
    pub limiter: Arc<SpeedLimiter>,
    pub events: broadcast::Sender<EngineEvent>,
    pub config: EngineConfig,
}

#[derive(Debug)]
pub enum Outcome {
    Completed { path: PathBuf },
    Stopped,
    Failed(OdmError),
}

type Joined = (ChunkRef, Result<(), ChunkFailure>);

/// Serializes "pick unique final name + rename" across all downloads in the process.
static FINISH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn build_client(cfg: &EngineConfig) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(cfg.connect_timeout)
        .read_timeout(cfg.read_timeout)
        .user_agent(cfg.user_agent.clone())
        .build()
        .expect("reqwest client")
}

pub fn log_url(url: &str) -> &str {
    url.split(['?', '#']).next().unwrap_or(url)
}

pub fn part_path(rec: &DownloadRecord) -> PathBuf {
    let short = &rec.id[..rec.id.len().min(8)];
    Path::new(&rec.save_dir).join(format!("{}.{short}.odm-part", rec.filename))
}

pub fn header_map(h: &[(String, String)]) -> HeaderMap {
    let mut m = HeaderMap::new();
    for (k, v) in h {
        if let (Ok(k), Ok(v)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
            m.append(k, v);
        }
    }
    m
}

fn store_err(e: StoreError) -> OdmError {
    OdmError::new(ErrorKind::Other, format!("store: {e}"))
}

fn cancelled() -> OdmError {
    OdmError::new(ErrorKind::Cancelled, "cancelled")
}

fn validator_of(rec: &DownloadRecord) -> Option<String> {
    rec.etag.clone().filter(|e| !e.starts_with("W/")).or_else(|| rec.last_modified.clone())
}

pub async fn transition(env: &DownloadEnv, id: &str, state: DownloadState, err: Option<&OdmError>) {
    let pair = err.map(|e| (e.kind.code(), e.message.clone()));
    if let Err(e) = env.store.set_state(id, state, pair.clone()).await {
        tracing::error!(%id, "failed to persist state: {e}");
    }
    let _ = env.events.send(EngineEvent::State {
        id: id.to_string(),
        state,
        error_code: pair.as_ref().map(|p| p.0.clone()),
        error_message: pair.map(|p| p.1),
    });
}

pub async fn run_download(env: &DownloadEnv, id: &str, cancel: CancellationToken) -> Outcome {
    match run_inner(env, id, &cancel).await {
        Ok(path) => Outcome::Completed { path },
        Err(_) if cancel.is_cancelled() => Outcome::Stopped,
        Err(e) => Outcome::Failed(e),
    }
}

async fn load(env: &DownloadEnv, id: &str) -> Result<DownloadRecord, OdmError> {
    env.store
        .get_download(id)
        .await
        .map_err(store_err)?
        .ok_or_else(|| OdmError::new(ErrorKind::Other, "download not found"))
}

async fn run_inner(env: &DownloadEnv, id: &str, cancel: &CancellationToken) -> Result<PathBuf, OdmError> {
    let mut restarted = false;
    loop {
        let mut rec = load(env, id).await?;
        let headers = header_map(&env.store.get_headers(id).await.map_err(store_err)?);
        let mut chunks = env.store.get_chunks(id).await.map_err(store_err)?;
        let mut require_partial = chunks.len() > 1;

        if chunks.is_empty() {
            transition(env, id, DownloadState::Probing, None).await;
            let p = probe_with_retry(env, &rec.url, &headers, cancel).await?;
            env.store
                .update_probe(id, ProbeMeta {
                    final_url: Some(p.final_url.clone()),
                    size: p.size,
                    mime: p.mime.clone(),
                    etag: p.etag.clone(),
                    last_modified: p.last_modified.clone(),
                    filename: p.filename.clone(),
                })
                .await
                .map_err(store_err)?;
            rec = load(env, id).await?;
            require_partial = p.supports_range && p.size.is_some();
            chunks = plan_chunks(p.size, require_partial, rec.max_connections);
            env.store.replace_chunks(id, chunks.clone()).await.map_err(store_err)?;
        }

        transition(env, id, DownloadState::Downloading, None).await;
        std::fs::create_dir_all(&rec.save_dir).map_err(io_err)?;
        let part = part_path(&rec);
        tracing::info!(%id, url = log_url(&rec.url), part = %part.display(), "downloading");

        if rec.size == Some(0) {
            File::create(&part).map_err(io_err)?;
            return finish(env, &rec, &part).await;
        }

        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&part)
            .map_err(io_err)?;
        if let Some(size) = rec.size {
            if file.metadata().map_err(io_err)?.len() != size {
                file.set_len(size).map_err(io_err)?;
            }
        }

        match transfer(env, &rec, Arc::new(file), &chunks, require_partial, cancel).await {
            Ok(()) => return finish(env, &rec, &part).await,
            Err(e)
                if !restarted
                    && !cancel.is_cancelled()
                    && matches!(e.kind, ErrorKind::RangeIgnored | ErrorKind::FileChanged) =>
            {
                tracing::warn!(%id, "restarting from zero: {e}");
                let _ = std::fs::remove_file(&part);
                let next = if e.kind == ErrorKind::RangeIgnored {
                    vec![ChunkRecord { idx: 0, start: 0, end: rec.size.map(|s| s - 1), written: 0 }]
                } else {
                    Vec::new() // forces a fresh probe
                };
                env.store.replace_chunks(id, next).await.map_err(store_err)?;
                restarted = true;
            }
            Err(e) => return Err(e),
        }
    }
}

async fn probe_with_retry(
    env: &DownloadEnv,
    url: &str,
    headers: &HeaderMap,
    cancel: &CancellationToken,
) -> Result<ProbeResult, OdmError> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let r = tokio::select! {
            _ = cancel.cancelled() => return Err(cancelled()),
            r = probe(&env.client, url, headers) => r,
        };
        match r {
            Ok(p) => return Ok(p),
            Err(e) if e.kind.is_transient() && attempt < env.config.max_attempts => {
                tokio::select! {
                    _ = cancel.cancelled() => return Err(cancelled()),
                    _ = tokio::time::sleep(backoff_delay(attempt, None)) => {}
                }
            }
            Err(e) => return Err(e),
        }
    }
}

fn spawn_worker(set: &mut JoinSet<Joined>, ctx: &ChunkCtx, chunk: ChunkRef, delay: Duration) {
    let ctx = ctx.clone();
    set.spawn(async move {
        if !delay.is_zero() {
            tokio::select! {
                _ = ctx.cancel.cancelled() => return (chunk, Err(cancelled().into())),
                _ = tokio::time::sleep(delay) => {}
            }
        }
        let r = run_chunk(&ctx, &chunk).await;
        (chunk, r)
    });
}

async fn transfer(
    env: &DownloadEnv,
    rec: &DownloadRecord,
    file: Arc<File>,
    chunks: &[ChunkRecord],
    require_partial: bool,
    cancel: &CancellationToken,
) -> Result<(), OdmError> {
    let cfg = &env.config;
    let id = rec.id.as_str();
    let mut table = ChunkTable::from_records(chunks);
    let work = cancel.child_token();
    let session = Arc::new(AtomicU64::new(0));
    let ctx = ChunkCtx {
        client: env.client.clone(),
        url: rec.final_url.clone().unwrap_or_else(|| rec.url.clone()),
        headers: header_map(&env.store.get_headers(id).await.map_err(store_err)?),
        validator: validator_of(rec),
        file,
        global_limit: env.limiter.clone(),
        local_limit: rec.speed_limit.map(|b| Arc::new(SpeedLimiter::new(b))),
        cancel: work.clone(),
        require_partial,
        session_bytes: session.clone(),
    };
    let max_conn = rec.max_connections.clamp(1, 32) as usize;

    let mut set: JoinSet<Joined> = JoinSet::new();
    let mut layout_dirty = false;
    for c in table.pending() {
        spawn_worker(&mut set, &ctx, c, Duration::ZERO);
    }
    if require_partial {
        while set.len() < max_conn {
            let Some(c) = table.split_largest() else { break };
            layout_dirty = true;
            spawn_worker(&mut set, &ctx, c, Duration::ZERO);
        }
    }

    // idx -> (failures since last progress, written at last failure)
    let mut attempts: HashMap<u32, (u32, u64)> = HashMap::new();
    let mut meter = SpeedMeter::new(Duration::from_secs(5));
    let mut tick = tokio::time::interval(cfg.progress_interval);
    let mut last_persist = Instant::now();
    let mut fatal: Option<OdmError> = None;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                emit_progress(env, rec, &table, &mut meter, &session);
                if last_persist.elapsed() >= cfg.persist_interval {
                    persist(env, id, &ctx.file, &table, &mut layout_dirty).await?;
                    last_persist = Instant::now();
                }
            }
            joined = set.join_next() => {
                let Some(joined) = joined else { break };
                let (chunk, res) = joined
                    .map_err(|e| OdmError::new(ErrorKind::Other, format!("worker panicked: {e}")))?;
                match res {
                    Ok(()) => {
                        if require_partial && !work.is_cancelled() && set.len() < max_conn {
                            if let Some(c) = table.split_largest() {
                                layout_dirty = true;
                                spawn_worker(&mut set, &ctx, c, Duration::ZERO);
                            }
                        }
                    }
                    Err(f) if f.error.kind == ErrorKind::Cancelled => {}
                    Err(f) if f.error.kind.is_transient() && fatal.is_none() => {
                        let (idx, written) = { let c = chunk.lock().unwrap(); (c.idx, c.written) };
                        let entry = attempts.entry(idx).or_insert((0, written));
                        if written > entry.1 {
                            *entry = (0, written); // made progress: reset the budget
                        }
                        entry.0 += 1;
                        if entry.0 < cfg.max_attempts {
                            tracing::debug!(%id, idx, attempt = entry.0, "retrying chunk: {}", f.error);
                            spawn_worker(&mut set, &ctx, chunk, backoff_delay(entry.0, f.retry_after));
                        } else {
                            fatal = Some(f.error);
                            work.cancel();
                        }
                    }
                    Err(f) => {
                        if fatal.is_none() {
                            fatal = Some(f.error);
                        }
                        work.cancel();
                    }
                }
            }
        }
    }

    let mut force = true;
    persist(env, id, &ctx.file, &table, &mut force).await?;
    emit_progress(env, rec, &table, &mut meter, &session);

    if let Some(e) = fatal {
        return Err(e);
    }
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    if let Some(size) = rec.size {
        let total = table.total_written();
        if total != size {
            return Err(OdmError::new(ErrorKind::Other, format!("incomplete: {total} of {size} bytes")));
        }
    }
    Ok(())
}

async fn persist(
    env: &DownloadEnv,
    id: &str,
    file: &Arc<File>,
    table: &ChunkTable,
    layout_dirty: &mut bool,
) -> Result<(), OdmError> {
    // Snapshot first, then fsync: every byte counted in the snapshot is then on disk.
    let snap = table.snapshot();
    let f = file.clone();
    tokio::task::spawn_blocking(move || f.sync_data())
        .await
        .map_err(|e| OdmError::new(ErrorKind::Other, e.to_string()))?
        .map_err(io_err)?;
    let r = if *layout_dirty {
        env.store.replace_chunks(id, snap).await
    } else {
        env.store.save_progress(id, snap.iter().map(|c| (c.idx, c.written)).collect()).await
    };
    r.map_err(store_err)?;
    *layout_dirty = false;
    Ok(())
}

fn emit_progress(env: &DownloadEnv, rec: &DownloadRecord, table: &ChunkTable, meter: &mut SpeedMeter, session: &AtomicU64) {
    meter.record(Instant::now(), session.load(Ordering::Relaxed));
    let bytes = table.total_written();
    let speed = meter.bps();
    let _ = env.events.send(EngineEvent::Progress(ProgressSnapshot {
        id: rec.id.clone(),
        bytes,
        total: rec.size,
        speed_bps: speed,
        eta_s: eta(rec.size, bytes, speed),
        chunks: table
            .snapshot()
            .into_iter()
            .map(|c| ChunkView { start: c.start, end: c.end, written: c.written })
            .collect(),
    }));
}

fn file_digest<D: Digest>(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut f = File::open(path)?;
    let mut h = D::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().to_vec())
}

fn verify_checksum(path: &Path, spec: &str) -> Result<(), OdmError> {
    let (algo, want) = spec
        .split_once(':')
        .ok_or_else(|| OdmError::new(ErrorKind::Other, "checksum must be algo:hex"))?;
    let got = match algo.to_ascii_lowercase().as_str() {
        "sha256" => file_digest::<Sha256>(path),
        "md5" => file_digest::<Md5>(path),
        other => return Err(OdmError::new(ErrorKind::Other, format!("unsupported checksum {other}"))),
    }
    .map_err(io_err)?;
    let got_hex: String = got.iter().map(|b| format!("{b:02x}")).collect();
    if got_hex.eq_ignore_ascii_case(want.trim()) {
        Ok(())
    } else {
        Err(OdmError::new(ErrorKind::ChecksumMismatch, format!("expected {want}, got {got_hex}")))
    }
}

async fn finish(env: &DownloadEnv, rec: &DownloadRecord, part: &Path) -> Result<PathBuf, OdmError> {
    if let Some(spec) = rec.checksum.clone() {
        let p = part.to_path_buf();
        tokio::task::spawn_blocking(move || verify_checksum(&p, &spec))
            .await
            .map_err(|e| OdmError::new(ErrorKind::Other, e.to_string()))??;
    }
    let dir = Path::new(&rec.save_dir);
    // std::fs::rename REPLACES an existing target on Windows, so choosing the
    // name and renaming must be atomic with respect to other finishing downloads.
    let (name, final_path) = {
        let _guard = FINISH_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let name = filename::unique_name(&rec.filename, |n| dir.join(n).exists());
        let final_path = dir.join(&name);
        std::fs::rename(part, &final_path).map_err(io_err)?;
        (name, final_path)
    };
    if name != rec.filename {
        env.store.set_filename(&rec.id, name).await.map_err(store_err)?;
    }
    if rec.size.is_none() {
        let len = std::fs::metadata(&final_path).map_err(io_err)?.len();
        env.store
            .update_probe(&rec.id, ProbeMeta { size: Some(len), ..Default::default() })
            .await
            .map_err(store_err)?;
    }
    tracing::info!(id = %rec.id, path = %final_path.display(), "completed");
    Ok(final_path)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p odm-engine --test download`
Expected: 14 passed.

Troubleshooting:
- **`build_client` doesn't compile because `ClientBuilder::read_timeout` isn't found:** your resolved reqwest is older than the version that added it. Run `cargo update -p reqwest`. Do not replace it with `.timeout()`, which caps the *whole* request and would kill large downloads.
- **`range_ignored_falls_back_to_single_connection` hangs:** check that the single fallback chunk gives `require_partial = false`, so the worker sends no Range header at offset 0.
- **`changed_server_file_restarts_from_zero` fails with FileChanged:** check that the restart branch empties the chunk list (which forces a new probe) and that `restarted` is only set once.
- **`resume_fills_free_connections_by_splitting` shows only 2 chunks:** check that the start-up split loop runs when `require_partial` is true, and that `persist` with `force = true` writes the layout.

- [ ] **Step 5: Run the whole engine suite**

Run: `cargo test -p odm-engine`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/odm-engine
git commit -m "feat(engine): download task with splitting, retries, resume and checksum"
```

---

### Task 13: `odm-engine`: manager actor (queue, concurrency, pause/resume/cancel/remove)

**Files:**
- Create: `crates/odm-engine/src/manager.rs`
- Modify: `crates/odm-engine/src/lib.rs` (add `pub mod manager; pub use manager::{AddRequest, ManagerHandle};`)
- Test: `crates/odm-engine/tests/manager.rs`

**Interfaces:**
- Consumes: `download::{run_download, transition, part_path, build_client, DownloadEnv, Outcome}`, `EngineConfig`, `SpeedLimiter`, `odm_core::{filename, category}`
- Produces:
  - `#[derive(Debug, Clone, Default)] pub struct AddRequest { pub url: String, pub filename: Option<String>, pub save_dir: Option<PathBuf>, pub category: Option<String>, pub headers: Vec<(String, String)>, pub max_connections: Option<u32>, pub checksum: Option<String>, pub page_url: Option<String>, pub page_title: Option<String>, pub priority: i64, pub start_paused: bool }` with `AddRequest::new(url)`
  - `#[derive(Clone)] pub struct ManagerHandle` with:
    - `async start(config: EngineConfig, store: Store) -> Result<ManagerHandle, OdmError>`: resets interrupted downloads to queued, then starts scheduling.
    - `async add(&self, AddRequest) -> Result<String, OdmError>`: returns the id. Rejects URLs that aren't `http(s)://`.
    - `async pause(&self, id: &str)`, `resume(&self, id: &str)`, `cancel(&self, id: &str, delete_file: bool)`, `remove(&self, id: &str, delete_file: bool)`, `pause_all()`, `resume_all()`, `set_speed_limit(bps: u64)`, `set_max_concurrent(n: usize)`. Each returns `Result<(), OdmError>` and errors only if the manager has stopped.
    - `async shutdown(&self)`: cancels everything and waits at most 3 s. Interrupted downloads are left `queued`.
    - `subscribe(&self) -> broadcast::Receiver<EngineEvent>`, `store(&self) -> &Store`
- **Guarantee:** a download is never started while its previous task is still running. Resuming a download whose pause is still in progress only takes effect once the task has finished.

- [ ] **Step 1: Write the failing tests**

`crates/odm-engine/tests/manager.rs`:
```rust
use odm_core::DownloadState;
use odm_engine::{AddRequest, EngineConfig, EngineEvent, ManagerHandle};
use odm_store::{DownloadRecord, Store};
use odm_testserver::{body, Faults, TestServer};
use std::path::Path;
use std::time::{Duration, Instant};

const MB: u64 = 1 << 20;

fn cfg(dir: &Path) -> EngineConfig {
    let mut c = EngineConfig::default();
    c.download_dir = dir.to_path_buf();
    c.progress_interval = Duration::from_millis(50);
    c.persist_interval = Duration::from_millis(200);
    c
}

async fn wait_state(store: &Store, id: &str, want: DownloadState, secs: u64) -> DownloadRecord {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let r = store.get_download(id).await.unwrap().expect("record");
        if r.state == want {
            return r;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {want:?}, state is {:?} ({:?})", r.state, r.error_msg);
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_bytes(store: &Store, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let w: u64 = store.get_chunks(id).await.unwrap().iter().map(|c| c.written).sum();
        if w > 0 {
            return;
        }
        assert!(Instant::now() < deadline, "no progress persisted");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn final_path(r: &DownloadRecord) -> std::path::PathBuf {
    Path::new(&r.save_dir).join(&r.filename)
}

#[tokio::test(flavor = "multi_thread")]
async fn completes_and_emits_events() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(2 * MB, Faults::default()).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap()).await.unwrap();
    let mut rx = m.subscribe();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed, 20).await;
    assert_eq!(std::fs::read(final_path(&r)).unwrap(), body(2 * MB));

    let mut seen = Vec::new();
    while let Ok(e) = rx.try_recv() {
        seen.push(e);
    }
    let states: Vec<DownloadState> = seen
        .iter()
        .filter_map(|e| match e { EngineEvent::State { id: i, state, .. } if *i == id => Some(*state), _ => None })
        .collect();
    assert!(matches!(seen.first(), Some(EngineEvent::Added { .. })));
    assert_eq!(states, vec![DownloadState::Probing, DownloadState::Downloading, DownloadState::Completed]);
    assert!(seen.iter().any(|e| matches!(e, EngineEvent::Progress(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn queue_respects_max_concurrent() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(MB, Faults { bytes_per_sec: Some(2 * MB), ..Default::default() }).await;
    let mut c = cfg(d.path());
    c.max_concurrent = 2;
    let m = ManagerHandle::start(c, Store::open_in_memory().unwrap()).await.unwrap();
    let mut ids = Vec::new();
    for _ in 0..4 {
        let mut r = AddRequest::new(s.url("/file"));
        r.max_connections = Some(1);
        ids.push(m.add(r).await.unwrap());
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut peak = 0;
    loop {
        let all = m.store().list_downloads().await.unwrap();
        let active = all.iter().filter(|r| r.state.is_active()).count();
        peak = peak.max(active);
        assert!(active <= 2, "{active} active");
        if all.iter().all(|r| r.state == DownloadState::Completed) {
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(peak, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_then_immediate_resume_is_safe() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(12 * MB, Faults { bytes_per_sec: Some(MB / 2), ..Default::default() }).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap()).await.unwrap();
    let mut rx = m.subscribe();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_bytes(m.store(), &id).await;
    m.pause(&id).await.unwrap();
    m.resume(&id).await.unwrap(); // arrives while the task is still stopping
    let r = wait_state(m.store(), &id, DownloadState::Completed, 60).await;
    assert_eq!(std::fs::read(final_path(&r)).unwrap(), body(12 * MB));

    let mut states = Vec::new();
    while let Ok(e) = rx.try_recv() {
        if let EngineEvent::State { state, .. } = e {
            states.push(state);
        }
    }
    let p = states.iter().position(|s| *s == DownloadState::Paused).expect("paused");
    assert_eq!(states[p + 1], DownloadState::Queued, "{states:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_with_delete_removes_part_file() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(12 * MB, Faults { bytes_per_sec: Some(MB / 2), ..Default::default() }).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap()).await.unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_bytes(m.store(), &id).await;
    m.cancel(&id, true).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Cancelled, 10).await;
    let leftovers: Vec<_> = std::fs::read_dir(d.path()).unwrap().collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn remove_deletes_record_and_file() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap()).await.unwrap();
    let mut rx = m.subscribe();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed, 10).await;
    m.remove(&id, true).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while m.store().get_download(&id).await.unwrap().is_some() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!final_path(&r).exists());
    let mut removed = false;
    while let Ok(e) = rx.try_recv() {
        removed |= matches!(e, EngineEvent::Removed { id: ref i } if *i == id);
    }
    assert!(removed);
}

#[tokio::test(flavor = "multi_thread")]
async fn same_url_twice_both_complete() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(3 * MB, Faults { bytes_per_sec: Some(4 * MB), ..Default::default() }).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap()).await.unwrap();
    let a = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let b = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let ra = wait_state(m.store(), &a, DownloadState::Completed, 30).await;
    let rb = wait_state(m.store(), &b, DownloadState::Completed, 30).await;
    let mut names = vec![ra.filename.clone(), rb.filename.clone()];
    names.sort();
    assert_eq!(names, vec!["file".to_string(), "file (1)".to_string()]);
    assert_eq!(std::fs::read(final_path(&ra)).unwrap(), body(3 * MB));
    assert_eq!(std::fs::read(final_path(&rb)).unwrap(), body(3 * MB));
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_then_resume_retries() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults { fail_status: Some(404), fail_times: 1, ..Default::default() }).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap()).await.unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Failed, 10).await;
    assert_eq!(r.error_kind.as_deref(), Some("http_404"));
    m.resume(&id).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Completed, 10).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_then_restart_resumes() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("odm.db");
    let s = TestServer::start(12 * MB, Faults { bytes_per_sec: Some(MB / 2), ..Default::default() }).await;
    let store = Store::open(&db).unwrap();
    let m = ManagerHandle::start(cfg(d.path()), store.clone()).await.unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_bytes(&store, &id).await;
    let t = Instant::now();
    m.shutdown().await;
    assert!(t.elapsed() < Duration::from_secs(4));
    assert_eq!(store.get_download(&id).await.unwrap().unwrap().state, DownloadState::Queued);
    assert!(m.pause(&id).await.is_err(), "commands after shutdown must error");

    let m2 = ManagerHandle::start(cfg(d.path()), store.clone()).await.unwrap();
    let r = wait_state(m2.store(), &id, DownloadState::Completed, 60).await;
    assert_eq!(std::fs::read(final_path(&r)).unwrap(), body(12 * MB));
}

#[tokio::test(flavor = "multi_thread")]
async fn start_paused_and_bad_urls() {
    let d = tempfile::tempdir().unwrap();
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap()).await.unwrap();
    let mut r = AddRequest::new("http://127.0.0.1:9/never");
    r.start_paused = true;
    let id = m.add(r).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(m.store().get_download(&id).await.unwrap().unwrap().state, DownloadState::Paused);
    assert!(m.add(AddRequest::new("ftp://x/y")).await.is_err());
    assert!(m.add(AddRequest::new("javascript:alert(1)")).await.is_err());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-engine --test manager`
Expected: compile error, "unresolved import `odm_engine::AddRequest`".

- [ ] **Step 3: Implement `src/manager.rs`**

```rust
use odm_core::{category, filename, DownloadState, ErrorKind, OdmError};
use odm_store::{DownloadRecord, Store};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::download::{build_client, part_path, run_download, transition, DownloadEnv, Outcome};
use crate::{EngineConfig, EngineEvent, SpeedLimiter};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Default)]
pub struct AddRequest {
    pub url: String,
    pub filename: Option<String>,
    pub save_dir: Option<PathBuf>,
    pub category: Option<String>,
    pub headers: Vec<(String, String)>,
    pub max_connections: Option<u32>,
    pub checksum: Option<String>,
    pub page_url: Option<String>,
    pub page_title: Option<String>,
    pub priority: i64,
    pub start_paused: bool,
}

impl AddRequest {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into(), ..Default::default() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stop {
    Pause,
    Cancel { delete: bool },
    Remove { delete: bool },
    Shutdown,
}

enum Cmd {
    Add(AddRequest, oneshot::Sender<Result<String, OdmError>>),
    Pause(String),
    Resume(String),
    Cancel(String, bool),
    Remove(String, bool),
    PauseAll,
    ResumeAll,
    SetSpeedLimit(u64),
    SetMaxConcurrent(usize),
    Finished(String, Outcome),
    Shutdown(oneshot::Sender<()>),
    ShutdownDeadline,
}

struct Active {
    cancel: CancellationToken,
    stop: Option<Stop>,
}

struct Manager {
    env: DownloadEnv,
    tx: mpsc::Sender<Cmd>,
    active: HashMap<String, Active>,
    resume_after: HashSet<String>,
    shutdown_reply: Option<oneshot::Sender<()>>,
    max_concurrent: usize,
}

#[derive(Clone)]
pub struct ManagerHandle {
    tx: mpsc::Sender<Cmd>,
    events: broadcast::Sender<EngineEvent>,
    store: Store,
}

fn closed() -> OdmError {
    OdmError::new(ErrorKind::Other, "engine stopped")
}

impl ManagerHandle {
    pub async fn start(config: EngineConfig, store: Store) -> Result<ManagerHandle, OdmError> {
        store
            .reset_interrupted()
            .await
            .map_err(|e| OdmError::new(ErrorKind::Other, format!("store: {e}")))?;
        let (events, _) = broadcast::channel(4096);
        let env = DownloadEnv {
            store: store.clone(),
            client: build_client(&config),
            limiter: Arc::new(SpeedLimiter::new(config.speed_limit_bps)),
            events: events.clone(),
            config: config.clone(),
        };
        let (tx, rx) = mpsc::channel(256);
        let manager = Manager {
            env,
            tx: tx.clone(),
            active: HashMap::new(),
            resume_after: HashSet::new(),
            shutdown_reply: None,
            max_concurrent: config.max_concurrent.clamp(1, 10),
        };
        tokio::spawn(manager.run(rx));
        Ok(ManagerHandle { tx, events, store })
    }

    async fn send(&self, c: Cmd) -> Result<(), OdmError> {
        self.tx.send(c).await.map_err(|_| closed())
    }

    pub async fn add(&self, req: AddRequest) -> Result<String, OdmError> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::Add(req, tx)).await?;
        rx.await.map_err(|_| closed())?
    }
    pub async fn pause(&self, id: &str) -> Result<(), OdmError> { self.send(Cmd::Pause(id.into())).await }
    pub async fn resume(&self, id: &str) -> Result<(), OdmError> { self.send(Cmd::Resume(id.into())).await }
    pub async fn cancel(&self, id: &str, delete_file: bool) -> Result<(), OdmError> {
        self.send(Cmd::Cancel(id.into(), delete_file)).await
    }
    pub async fn remove(&self, id: &str, delete_file: bool) -> Result<(), OdmError> {
        self.send(Cmd::Remove(id.into(), delete_file)).await
    }
    pub async fn pause_all(&self) -> Result<(), OdmError> { self.send(Cmd::PauseAll).await }
    pub async fn resume_all(&self) -> Result<(), OdmError> { self.send(Cmd::ResumeAll).await }
    pub async fn set_speed_limit(&self, bps: u64) -> Result<(), OdmError> { self.send(Cmd::SetSpeedLimit(bps)).await }
    pub async fn set_max_concurrent(&self, n: usize) -> Result<(), OdmError> {
        self.send(Cmd::SetMaxConcurrent(n)).await
    }

    pub async fn shutdown(&self) {
        let (tx, rx) = oneshot::channel();
        if self.send(Cmd::Shutdown(tx)).await.is_ok() {
            let _ = tokio::time::timeout(SHUTDOWN_GRACE + Duration::from_secs(1), rx).await;
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.events.subscribe()
    }

    pub fn store(&self) -> &Store {
        &self.store
    }
}

impl Manager {
    async fn run(mut self, mut rx: mpsc::Receiver<Cmd>) {
        self.schedule().await;
        while let Some(cmd) = rx.recv().await {
            match cmd {
                Cmd::Add(req, reply) => {
                    let r = self.add(req).await;
                    let _ = reply.send(r);
                }
                Cmd::Pause(id) => self.pause(&id).await,
                Cmd::Resume(id) => self.resume(&id).await,
                Cmd::Cancel(id, delete) => self.cancel(&id, delete).await,
                Cmd::Remove(id, delete) => self.remove(&id, delete).await,
                Cmd::PauseAll => {
                    for id in self.ids_in(&[DownloadState::Queued]).await.into_iter().chain(self.active.keys().cloned().collect::<Vec<_>>()) {
                        self.pause(&id).await;
                    }
                }
                Cmd::ResumeAll => {
                    for id in self.ids_in(&[DownloadState::Paused]).await.into_iter().chain(self.active.keys().cloned().collect::<Vec<_>>()) {
                        self.resume(&id).await;
                    }
                }
                Cmd::SetSpeedLimit(bps) => self.env.limiter.set_rate(bps),
                Cmd::SetMaxConcurrent(n) => {
                    self.max_concurrent = n.clamp(1, 10);
                    self.schedule().await;
                }
                Cmd::Finished(id, outcome) => self.finished(id, outcome).await,
                Cmd::Shutdown(reply) => {
                    self.shutdown_reply = Some(reply);
                    for a in self.active.values_mut() {
                        a.stop = Some(Stop::Shutdown);
                        a.cancel.cancel();
                    }
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(SHUTDOWN_GRACE).await;
                        let _ = tx.send(Cmd::ShutdownDeadline).await;
                    });
                }
                Cmd::ShutdownDeadline => {
                    if let Some(r) = self.shutdown_reply.take() {
                        let _ = r.send(());
                    }
                    break;
                }
            }
            if self.shutdown_reply.is_some() && self.active.is_empty() {
                let _ = self.shutdown_reply.take().unwrap().send(());
                break;
            }
        }
        // rx is dropped here: further handle calls return Err(closed()).
    }

    async fn ids_in(&self, states: &[DownloadState]) -> Vec<String> {
        let mut out = Vec::new();
        for s in states {
            if let Ok(v) = self.env.store.list_by_state(*s).await {
                out.extend(v.into_iter().map(|r| r.id));
            }
        }
        out
    }

    async fn state_of(&self, id: &str) -> Option<DownloadState> {
        self.env.store.get_download(id).await.ok().flatten().map(|r| r.state)
    }

    async fn add(&mut self, req: AddRequest) -> Result<String, OdmError> {
        if !(req.url.starts_with("http://") || req.url.starts_with("https://")) {
            return Err(OdmError::new(ErrorKind::Other, "only http(s) URLs are supported"));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let name = req
            .filename
            .as_deref()
            .map(filename::sanitize)
            .or_else(|| filename::from_url(&req.url))
            .unwrap_or_else(|| "download".into());
        let cat = req
            .category
            .clone()
            .unwrap_or_else(|| category::detect(&name, None).as_str().to_string());
        let dir = req.save_dir.clone().unwrap_or_else(|| self.env.config.save_dir_for(&cat));

        let mut rec = DownloadRecord::new_file(&id, &req.url, &name, &dir.to_string_lossy());
        rec.category = cat;
        rec.filename_locked = req.filename.is_some();
        rec.max_connections = req.max_connections.unwrap_or(self.env.config.max_connections).clamp(1, 32);
        rec.checksum = req.checksum;
        rec.page_url = req.page_url;
        rec.page_title = req.page_title;
        rec.priority = req.priority;
        if req.start_paused {
            rec.state = DownloadState::Paused;
        }
        let store_err = |e| OdmError::new(ErrorKind::Other, format!("store: {e}"));
        self.env.store.insert_download(rec).await.map_err(store_err)?;
        if !req.headers.is_empty() {
            self.env.store.set_headers(&id, req.headers).await.map_err(store_err)?;
        }
        let _ = self.env.events.send(EngineEvent::Added { id: id.clone() });
        if !req.start_paused {
            self.schedule().await;
        }
        Ok(id)
    }

    fn request_stop(&mut self, id: &str, stop: Stop) -> bool {
        match self.active.get_mut(id) {
            Some(a) => {
                // Remove/Cancel outrank Pause; never downgrade.
                let keep = matches!(a.stop, Some(Stop::Remove { .. } | Stop::Cancel { .. })) && stop == Stop::Pause;
                if !keep {
                    a.stop = Some(stop);
                }
                a.cancel.cancel();
                true
            }
            None => false,
        }
    }

    async fn pause(&mut self, id: &str) {
        self.resume_after.remove(id);
        if !self.request_stop(id, Stop::Pause) && self.state_of(id).await == Some(DownloadState::Queued) {
            transition(&self.env, id, DownloadState::Paused, None).await;
        }
    }

    async fn resume(&mut self, id: &str) {
        if let Some(a) = self.active.get(id) {
            if a.stop == Some(Stop::Pause) {
                self.resume_after.insert(id.to_string());
            }
            return;
        }
        if matches!(self.state_of(id).await, Some(DownloadState::Paused | DownloadState::Failed)) {
            transition(&self.env, id, DownloadState::Queued, None).await;
            self.schedule().await;
        }
    }

    async fn cancel(&mut self, id: &str, delete: bool) {
        self.resume_after.remove(id);
        if self.request_stop(id, Stop::Cancel { delete }) {
            return;
        }
        if let Some(s) = self.state_of(id).await {
            if !s.is_terminal() || s == DownloadState::Failed {
                if delete {
                    self.delete_part(id).await;
                }
                transition(&self.env, id, DownloadState::Cancelled, None).await;
            }
        }
    }

    async fn remove(&mut self, id: &str, delete: bool) {
        self.resume_after.remove(id);
        if !self.request_stop(id, Stop::Remove { delete }) {
            self.delete_record(id, delete).await;
        }
    }

    async fn delete_part(&self, id: &str) {
        if let Ok(Some(rec)) = self.env.store.get_download(id).await {
            let _ = std::fs::remove_file(part_path(&rec));
        }
    }

    async fn delete_record(&self, id: &str, delete_file: bool) {
        if let Ok(Some(rec)) = self.env.store.get_download(id).await {
            if delete_file {
                let _ = std::fs::remove_file(part_path(&rec));
                if rec.state == DownloadState::Completed {
                    let _ = std::fs::remove_file(Path::new(&rec.save_dir).join(&rec.filename));
                }
            }
        }
        if self.env.store.delete_download(id).await.is_ok() {
            let _ = self.env.events.send(EngineEvent::Removed { id: id.to_string() });
        }
    }

    async fn finished(&mut self, id: String, outcome: Outcome) {
        let stop = self.active.remove(&id).and_then(|a| a.stop);
        match (stop, outcome) {
            (Some(Stop::Remove { delete }), Outcome::Completed { .. }) => {
                transition(&self.env, &id, DownloadState::Completed, None).await;
                self.delete_record(&id, delete).await;
            }
            (Some(Stop::Remove { delete }), _) => self.delete_record(&id, delete).await,
            (_, Outcome::Completed { .. }) => transition(&self.env, &id, DownloadState::Completed, None).await,
            (Some(Stop::Pause), _) => transition(&self.env, &id, DownloadState::Paused, None).await,
            (Some(Stop::Cancel { delete }), _) => {
                if delete {
                    self.delete_part(&id).await;
                }
                transition(&self.env, &id, DownloadState::Cancelled, None).await;
            }
            (Some(Stop::Shutdown), _) => transition(&self.env, &id, DownloadState::Queued, None).await,
            (None, Outcome::Failed(e)) => {
                tracing::warn!(%id, "download failed: {e}");
                transition(&self.env, &id, DownloadState::Failed, Some(&e)).await;
            }
            (None, Outcome::Stopped) => transition(&self.env, &id, DownloadState::Paused, None).await,
        }
        if self.resume_after.remove(&id) && self.state_of(&id).await == Some(DownloadState::Paused) {
            transition(&self.env, &id, DownloadState::Queued, None).await;
        }
        self.schedule().await;
    }

    async fn schedule(&mut self) {
        if self.shutdown_reply.is_some() {
            return;
        }
        let free = self.max_concurrent.saturating_sub(self.active.len());
        if free == 0 {
            return;
        }
        let queued = match self.env.store.list_by_state(DownloadState::Queued).await {
            Ok(q) => q,
            Err(e) => {
                tracing::error!("schedule: {e}");
                return;
            }
        };
        let ids: Vec<String> = queued
            .into_iter()
            .map(|r| r.id)
            .filter(|id| !self.active.contains_key(id))
            .take(free)
            .collect();
        for id in ids {
            self.launch(id);
        }
    }

    fn launch(&mut self, id: String) {
        let cancel = CancellationToken::new();
        let env = self.env.clone();
        let tx = self.tx.clone();
        let (task_id, task_cancel) = (id.clone(), cancel.clone());
        tokio::spawn(async move {
            let out = run_download(&env, &task_id, task_cancel).await;
            let _ = tx.send(Cmd::Finished(task_id, out)).await;
        });
        self.active.insert(id, Active { cancel, stop: None });
    }
}
```

Notes for the implementer:
- `Outcome::Completed { path }` is matched with `{ .. }`, so the manager doesn't need the path. The UI reads `save_dir` + `filename` from the store.
- `transition` to `Queued` from `Paused` passes through the store without validation. State-machine validation (`can_transition`) is only asserted in tests. **Do not add** checks that could reject the shutdown path `Downloading → Queued`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p odm-engine --test manager`
Expected: 9 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/odm-engine
git commit -m "feat(engine): manager actor with queue, limits and safe pause/resume"
```

---

### Task 14: Regression suite for the old audit's failures

**Files:**
- Test: `crates/odm-engine/tests/regression.rs`

**Interfaces:**
- Consumes: `ManagerHandle`, `AddRequest`, `EngineConfig`, `Store`, `TestServer`, `odm_engine::download::part_path`

(Unknown size, a tiny speed limit, a server that ignores Range, and pause→resume are already covered in Tasks 5, 12 and 13. This task adds the three cases that need a whole manager.)

- [ ] **Step 1: Write the tests**

```rust
use odm_core::DownloadState;
use odm_engine::download::part_path;
use odm_engine::{AddRequest, EngineConfig, ManagerHandle};
use odm_store::Store;
use odm_testserver::{body, Faults, TestServer};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::{Duration, Instant};

const MB: u64 = 1 << 20;

fn cfg(dir: &Path) -> EngineConfig {
    let mut c = EngineConfig::default();
    c.download_dir = dir.to_path_buf();
    c.persist_interval = Duration::from_millis(200);
    c
}

/// Regression: the old engine deadlocked when controllers + chunks exceeded the pool.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn many_downloads_many_connections_no_deadlock() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(8 * MB, Faults::default()).await;
    let mut c = cfg(d.path());
    c.max_concurrent = 10;
    c.max_connections = 32;
    let m = ManagerHandle::start(c, Store::open_in_memory().unwrap()).await.unwrap();
    let mut ids = Vec::new();
    for _ in 0..32 {
        ids.push(m.add(AddRequest::new(s.url("/file"))).await.unwrap());
    }
    let res = tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let all = m.store().list_downloads().await.unwrap();
            if all.iter().all(|r| r.state == DownloadState::Completed) {
                break all;
            }
            assert!(all.iter().all(|r| r.state != DownloadState::Failed), "a download failed");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("deadlock: downloads did not finish in 120 s");
    let want = Sha256::digest(body(8 * MB)).to_vec();
    for r in res {
        let got = Sha256::digest(std::fs::read(Path::new(&r.save_dir).join(&r.filename)).unwrap()).to_vec();
        assert_eq!(got, want, "{}", r.filename);
    }
}

/// Regression: kill the process mid-download (no graceful persist), restart, compare hashes.
#[test]
fn crash_mid_download_then_resume_is_identical() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("odm.db");
    let store = Store::open(&db).unwrap();

    // Server lives in its own runtime so it survives the "crash".
    let server_rt = tokio::runtime::Runtime::new().unwrap();
    let s = server_rt.block_on(TestServer::start(8 * MB, Faults { bytes_per_sec: Some(MB), ..Default::default() }));
    let url = s.url("/file");

    // Runtime 1: start downloading, then drop it abruptly.
    let rt1 = tokio::runtime::Runtime::new().unwrap();
    let id = rt1.block_on(async {
        let m = ManagerHandle::start(cfg(d.path()), store.clone()).await.unwrap();
        let id = m.add(AddRequest::new(url.clone())).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let w: u64 = store.get_chunks(&id).await.unwrap().iter().map(|c| c.written).sum();
            if w > MB {
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        id
    });
    rt1.shutdown_background(); // abrupt: no final persist, no state change

    // Runtime 2: fresh manager over the same DB must finish with identical bytes.
    let rt2 = tokio::runtime::Runtime::new().unwrap();
    rt2.block_on(async {
        let m = ManagerHandle::start(cfg(d.path()), store.clone()).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let r = loop {
            let r = store.get_download(&id).await.unwrap().unwrap();
            if r.state == DownloadState::Completed {
                break r;
            }
            assert!(r.state != DownloadState::Failed, "{:?}", r.error_msg);
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        let got = Sha256::digest(std::fs::read(Path::new(&r.save_dir).join(&r.filename)).unwrap()).to_vec();
        assert_eq!(got, Sha256::digest(body(8 * MB)).to_vec());
        m.shutdown().await;
    });
    drop(s);
}

/// Regression: the old allocator produced a 1-byte file.
#[tokio::test(flavor = "multi_thread")]
async fn part_file_is_preallocated_to_full_size() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(8 * MB, Faults { bytes_per_sec: Some(MB), ..Default::default() }).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap()).await.unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let r = m.store().get_download(&id).await.unwrap().unwrap();
        if r.state == DownloadState::Downloading {
            let part = part_path(&r);
            if let Ok(meta) = std::fs::metadata(&part) {
                assert_eq!(meta.len(), 8 * MB);
                break;
            }
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    m.cancel(&id, true).await.unwrap();
}
```

- [ ] **Step 2: Run them**

Run: `cargo test -p odm-engine --test regression -- --test-threads=1`
Expected: 3 passed. The deadlock test writes 256 MB of temporary files and takes up to about 30 s.

If `crash_mid_download_then_resume_is_identical` fails with the state stuck at `downloading`: `ManagerHandle::start` must call `reset_interrupted()` before scheduling. Check Task 13, where `start` does this first.

- [ ] **Step 3: Commit**

```bash
git add crates/odm-engine/tests/regression.rs
git commit -m "test(engine): regression suite for deadlock, crash-resume, preallocation"
```

---

### Task 15: `odm-cli`: drive the engine from the terminal

**Files:**
- Create: `crates/odm-cli/Cargo.toml`, `crates/odm-cli/src/lib.rs`, `crates/odm-cli/src/main.rs`
- Test: `crates/odm-cli/tests/cli.rs`

**Interfaces:**
- Consumes: `ManagerHandle`, `AddRequest`, `EngineConfig`, `EngineEvent`, `Store`, `odm_core::ids::APP_DIR_NAME`
- Produces:
  - `pub struct GetArgs { pub url: String, pub dir: Option<PathBuf>, pub name: Option<String>, pub connections: Option<u32>, pub limit_bps: u64, pub db: Option<PathBuf>, pub quiet: bool }`
  - `pub async fn run_get(args: GetArgs) -> Result<PathBuf, String>`
  - The binary is `odm-cli get <URL> [--dir D] [--name N] [-c N] [--limit BPS] [--db PATH] [--quiet]`. It exits 0 on success and 1 on failure. The default database is `%APPDATA%\ODM\odm-cli.db`, kept separate from the app's `odm.db`.

- [ ] **Step 1: Manifest**

```toml
[package]
name = "odm-cli"
edition.workspace = true
version.workspace = true
license.workspace = true

[[bin]]
name = "odm-cli"
path = "src/main.rs"

[dependencies]
odm-core.workspace = true
odm-store.workspace = true
odm-engine.workspace = true
tokio.workspace = true
clap.workspace = true
tracing-subscriber.workspace = true

[dev-dependencies]
odm-testserver.workspace = true
tempfile.workspace = true
```

- [ ] **Step 2: Write the failing test**

`crates/odm-cli/tests/cli.rs`:
```rust
use odm_cli::{run_get, GetArgs};
use odm_testserver::{body, Faults, TestServer};

#[tokio::test(flavor = "multi_thread")]
async fn get_downloads_to_dir() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(3 << 20, Faults::default()).await;
    let path = run_get(GetArgs {
        url: s.url("/file"),
        dir: Some(d.path().to_path_buf()),
        name: Some("out.bin".into()),
        connections: Some(4),
        limit_bps: 0,
        db: Some(d.path().join("cli.db")),
        quiet: true,
    })
    .await
    .unwrap();
    assert_eq!(path, d.path().join("out.bin"));
    assert_eq!(std::fs::read(path).unwrap(), body(3 << 20));
}

#[tokio::test(flavor = "multi_thread")]
async fn get_reports_failure() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(10, Faults { fail_status: Some(404), fail_times: 99, ..Default::default() }).await;
    let err = run_get(GetArgs {
        url: s.url("/file"),
        dir: Some(d.path().to_path_buf()),
        name: None,
        connections: None,
        limit_bps: 0,
        db: Some(d.path().join("cli.db")),
        quiet: true,
    })
    .await
    .unwrap_err();
    assert!(err.contains("http_404"), "{err}");
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test -p odm-cli`
Expected: compile error, "can't find crate `odm_cli`" (there's no lib yet).

- [ ] **Step 4: Implement**

`src/lib.rs`:
```rust
use odm_core::DownloadState;
use odm_engine::{AddRequest, EngineConfig, EngineEvent, ManagerHandle};
use odm_store::Store;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct GetArgs {
    pub url: String,
    pub dir: Option<PathBuf>,
    pub name: Option<String>,
    pub connections: Option<u32>,
    pub limit_bps: u64,
    pub db: Option<PathBuf>,
    pub quiet: bool,
}

fn default_db() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join(odm_core::ids::APP_DIR_NAME).join("odm-cli.db")
}

fn human(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", U[i])
}

pub async fn run_get(args: GetArgs) -> Result<PathBuf, String> {
    let store = Store::open(&args.db.unwrap_or_else(default_db)).map_err(|e| e.to_string())?;
    let mut cfg = EngineConfig::default();
    cfg.speed_limit_bps = args.limit_bps;
    if let Some(d) = &args.dir {
        cfg.download_dir = d.clone();
    }
    let m = ManagerHandle::start(cfg, store.clone()).await.map_err(|e| e.to_string())?;
    let mut rx = m.subscribe();
    let mut req = AddRequest::new(args.url);
    req.filename = args.name;
    req.max_connections = args.connections;
    req.save_dir = args.dir;
    let id = m.add(req).await.map_err(|e| e.to_string())?;

    let result = loop {
        match rx.recv().await {
            Ok(EngineEvent::Progress(p)) if p.id == id && !args.quiet => {
                let total = p.total.map(human).unwrap_or_else(|| "?".into());
                print!("\r{} / {}  {}/s  {} conns   ", human(p.bytes), total, human(p.speed_bps), p.chunks.len());
                let _ = std::io::stdout().flush();
            }
            Ok(EngineEvent::State { id: i, state, error_code, error_message }) if i == id => match state {
                DownloadState::Completed => {
                    let r = store.get_download(&id).await.map_err(|e| e.to_string())?.ok_or("record vanished")?;
                    break Ok(Path::new(&r.save_dir).join(&r.filename));
                }
                DownloadState::Failed | DownloadState::Cancelled => {
                    break Err(format!(
                        "{}: {}",
                        error_code.unwrap_or_else(|| state.as_str().into()),
                        error_message.unwrap_or_default()
                    ));
                }
                _ => {}
            },
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            Err(e) => break Err(e.to_string()),
        }
    };
    if !args.quiet {
        println!();
    }
    m.shutdown().await;
    result
}
```

`src/main.rs`:
```rust
use clap::{Parser, Subcommand};
use odm_cli::{run_get, GetArgs};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "odm-cli", version, about = "ODM engine command-line driver")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Download a URL
    Get {
        url: String,
        #[arg(long)]
        dir: Option<PathBuf>,
        #[arg(long)]
        name: Option<String>,
        #[arg(short = 'c', long)]
        connections: Option<u32>,
        /// Speed limit in bytes per second (0 = unlimited)
        #[arg(long, default_value_t = 0)]
        limit: u64,
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        quiet: bool,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let Cmd::Get { url, dir, name, connections, limit, db, quiet } = Cli::parse().cmd;
    match run_get(GetArgs { url, dir, name, connections, limit_bps: limit, db, quiet }).await {
        Ok(p) => println!("saved {}", p.display()),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p odm-cli`
Expected: 2 passed.

- [ ] **Step 6: Manual smoke test against a real server**

Run: `cargo run -p odm-cli --release -- get "https://proof.ovh.net/files/100Mb.dat" --dir "$TEMP/odm-smoke" -c 8`
Expected: a live progress line showing several connections, then `saved …\100Mb.dat`. The file size should be 12,500,000 bytes (100 Mbit). If that host is unreachable, use any large file URL that supports ranges.

- [ ] **Step 7: Commit**

```bash
git add crates/odm-cli
git commit -m "feat(cli): odm-cli get for driving the engine from a terminal"
```

---

### Task 16: Whole-workspace verification

**Files:** none (verification only).

- [ ] **Step 1: Format and lint**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings. Fix any that appear, but don't disable lints.

- [ ] **Step 2: Full test suite**

Run: `cargo test --workspace -- --test-threads=4`
Expected: every test passes.

- [ ] **Step 3: Check that the audit's failures are covered**

Confirm that each of these tests exists and passes:

| Old bug | Test |
|---|---|
| 1-byte preallocation | `regression::part_file_is_preallocated_to_full_size` |
| Speed limiter hang | `limiter::request_larger_than_bucket_completes`, `limiter::tiny_limit_below_buffer_size_does_not_hang` |
| Unknown size crash | `download::unknown_size_download_completes` |
| Range ignored → corrupt file | `download::range_ignored_falls_back_to_single_connection`, `worker::stops_at_end_even_if_server_sends_more` |
| Pool deadlock | `regression::many_downloads_many_connections_no_deadlock` |
| No retries | `download::dropped_connections_are_retried` |
| Pause/resume double writer | `manager::pause_then_immediate_resume_is_safe` |
| Changed file stitched | `download::changed_server_file_restarts_from_zero` |
| Name overwrite / unsafe names | `download::name_collision_and_disposition`, `filename::*` |
| Crash resume | `regression::crash_mid_download_then_resume_is_identical` |

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "chore: clippy and fmt clean"
```

---

## What comes next (separate plans)

- **Plan 2: Tauri UI.** Commands wrapping `ManagerHandle`, a `progress-batch` event throttled to 4 per second, the Svelte table, the detail panel with the chunk map, dialogs, settings persistence (`settings.json`), tray, single instance and notifications.
- **Plan 3: Native host and extension.** The `odm-host` crate, the named-pipe server in the app, `--register`, and the TS extension with capture, context menu, popup and stream detection.
- **Plan 4: HLS.** A new `odm-engine::hls` module that reuses the limiter, retry logic and store tables from this plan.
- **Plan 5: Polish.** Installer bundle, first-run registration and README.

**Spec items deliberately deferred from this plan (don't drop them):**
- Spec §7 `settings.json` (atomic write, invalid fields fall back to defaults) and spec §10 rotating log files in `%APPDATA%\ODM\logs`: these are **Plan 2** (app shell). This plan only emits `tracing` events.
- Spec §5.5 "Link expired: a later `ADD_DOWNLOAD` for the same final filename offers to reuse the partial data with the new URL": this is **Plan 3**. It needs a `ManagerHandle::refresh_url(id, url, headers)` that swaps the URL and headers on a failed record and resumes it.
- Spec §5.8 refining the category from the probe's MIME type: this is **Plan 2**, where the UI shows categories.
