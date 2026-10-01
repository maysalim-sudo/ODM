# ODM Plan 2: Desktop App (Tauri UI) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the Plan 1 engine into a usable Windows desktop app: a Tauri 2 window with the download table, a detail panel with a live chunk map, add/batch/settings dialogs, tray, single instance, notifications, persisted settings and log files.

**Architecture:**
- A new Tauri-free crate, `odm-appcore`, holds everything testable: app paths, `settings.json`, logging, the DTOs sent to the UI, the functions behind each command, and the bridge that turns engine events into UI events (progress batched to 4 per second).
- `app/src-tauri` is a thin Tauri shell: it wires `odm-appcore` to Tauri commands, events, tray, window and plugins.
- `app/src` is a Svelte 5 + TypeScript UI. Its state logic lives in plain TS modules tested with vitest; components stay thin.
- Three small engine/store additions come first, because the UI needs them: runtime `set_config`, read-only queries with byte counts, and placing a download by its MIME type.

**Tech Stack:** Rust stable 1.98 (MSVC), Tauri 2 (+ plugins: single-instance, dialog, notification, opener), tokio, serde, tracing + tracing-appender; Node 24, pnpm 12, Vite, Svelte 5 (runes), TypeScript, vitest, svelte-check.

**Spec:** `docs/superpowers/specs/2026-09-29-odm-design.md` (sections 2, 5.6–5.8, 7, 9, 10). **Carry-over:** `docs/superpowers/plans/2026-10-01-odm-plan-1-carryover.md` (read the last ruling: it lists the deferred items this plan must pick up).

## Global Constraints

- Product name `ODM`; Tauri identifier `com.odm.app`; app data directory `%APPDATA%\ODM\` containing `odm.db`, `settings.json`, `logs\`. Rust reads these from `odm_core::ids` only.
- Crate direction: `odm-core` ← `odm-store` ← `odm-engine` ← `odm-appcore` ← `app/src-tauri`. `odm-appcore` must not depend on `tauri`.
- The UI never talks to the store. It calls Tauri commands and listens to one event, `odm://event`.
- Progress reaches the UI at most 4 times per second per download, batched into one `progress_batch` event.
- On `Lagged` from the engine's event channel, the bridge emits `resync` and the UI reloads the list.
- Settings ranges: `max_concurrent` 1–10 (default 3); `max_connections` 1–32 (default 8); `max_attempts` 1–20 (default 5); connect timeout 5–120 s (default 30); read timeout 10–300 s (default 60); speed limit in KB/s, 0 = unlimited. Invalid or unknown fields in `settings.json` fall back to defaults with a log warning. The file is written atomically (temp file, then rename).
- Settings changes apply without restarting the manager (`ManagerHandle::set_config`). Running downloads keep the config they started with.
- Logs: `%APPDATA%\ODM\logs\odm.<date>.log`, rotated daily, 7 files kept. Cookie and Authorization values are never logged; URLs are logged without query strings.
- Window: closing hides to tray when `close_to_tray` is on; Quit asks for confirmation if downloads are active; a second launch focuses the existing window.
- Theme: dark. Background `#0f0f14`, surface `#14141c`, border `#1e1e28`, text `#e0e0e6`, muted `#6b6b80`, accent `#6366f1`, accent-light `#818cf8`, error `#ef4444`, success `#22c55e`.
- UI copy is sentence case, plain words, no exclamation marks.
- Git: single branch `master`. Commit per task; the controller pushes to `origin` after each task's review passes.
- Shell commands run from the repo root `E:\Data From Desktop\Coding\ODM` in Git Bash with `export PATH="$HOME/.cargo/bin:$PATH"`. Frontend commands run from `app/`.
- The Tauri and plugin code in this plan was written from the Tauri 2 API but not compiled. Minimal fixes to make it compile are expected; the behaviour and the interfaces listed per task are binding.

## Review Focus

Inputs the spec implies but doesn't spell out. The owning task must test each one:
1. **`settings.json` is corrupt or half-written** (invalid JSON, wrong types, a truncated file). The app must start with defaults for the bad parts and keep the valid fields. *Test: Task 3 (`corrupt_file_falls_back_to_defaults`, `one_bad_field_keeps_the_rest`).*
2. **The database was written by a newer ODM.** Opening it must fail with a clear error, not run against an unknown schema. *Test: Task 1 (`newer_schema_is_rejected`).*
3. **A burst of progress events larger than the channel** (the UI was asleep). The bridge must emit `resync`, and the UI must end up showing the true state. *Test: Task 6 (`lagged_emits_resync`) and Task 10 (`resync_replaces_the_list`).*
4. **The user pastes text that isn't only URLs into Batch add** (blank lines, duplicates, `ftp://`, trailing spaces). Valid http(s) URLs are added once each; the rest are reported back, not silently dropped. *Test: Task 5 (`parse_urls_filters_and_dedupes`).*
5. **A download with an unknown total size** (no Content-Length). The table and detail panel must show bytes and speed without a percentage, and must not divide by zero. *Test: Task 10 (`percent_is_null_without_total`).*

---

## File Structure

```
ODM/
  Cargo.toml                         add "app/src-tauri" to workspace members; new workspace deps
  crates/
    odm-store/src/{migrations,models,repo,lib}.rs      Task 1 changes
    odm-engine/src/{config,manager,download,limiter}.rs  Task 2 changes
    odm-testserver/src/lib.rs        Task 2: `content_type` fault
    odm-appcore/
      Cargo.toml
      src/lib.rs                     re-exports
      src/paths.rs                   AppPaths (data dir, db, settings, logs)
      src/settings.rs                Settings: defaults, tolerant load, clamp, atomic save, to_engine_config
      src/logging.rs                 rolling file writer + init
      src/dto.rs                     DownloadDto, DetailDto, UiEvent
      src/api.rs                     functions behind each command (list, detail, add, batch, suggest, apply settings)
      src/bridge.rs                  engine events -> UiEvent, progress batching, resync
      tests/{settings,api,bridge}.rs
  app/
    package.json, pnpm-lock.yaml, vite.config.ts, svelte.config.js, tsconfig.json, index.html
    app-icon.svg                     source for `tauri icon`
    src/
      main.ts, App.svelte, app.css
      lib/types.ts                   TS mirrors of DownloadDto / UiEvent / Settings
      lib/format.ts                  bytes, speed, ETA, percent, date
      lib/model.ts                   pure list model: applyEvent, filters, counts, sort
      lib/api.ts                     invoke wrappers + event subscription
      lib/downloads.svelte.ts        runes store wrapping model.ts
      lib/*.test.ts                  vitest
      components/Modal.svelte, Sidebar.svelte, Toolbar.svelte, DownloadTable.svelte,
                 ProgressBar.svelte, DetailPanel.svelte, ChunkMap.svelte,
                 AddDialog.svelte, BatchDialog.svelte, SettingsDialog.svelte
    src-tauri/
      Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, icons/
      src/main.rs                    entry (windows_subsystem)
      src/lib.rs                     run(): builder, setup, state
      src/commands.rs                #[tauri::command] wrappers over odm_appcore::api
      src/shell.rs                   tray, window events, quit flow, notifications
  README.md                          Task 15
```

---

### Task 1: Store additions (newer-schema guard, auto-place column, atomic add, byte counts)

**Files:**
- Modify: `crates/odm-store/src/migrations.rs`, `src/models.rs`, `src/repo.rs`, `src/lib.rs`
- Test: `crates/odm-store/tests/store.rs` (add tests; update the one version assertion)

**Interfaces:**
- Produces:
  - `StoreError::NewerSchema { found: i64, supported: i64 }` (Display: `database schema {found} is newer than this app supports ({supported})`). `Store::open` returns it when `user_version > MIGRATIONS.len()`.
  - Migration V2: `ALTER TABLE downloads ADD COLUMN auto_place INTEGER NOT NULL DEFAULT 0;` so `user_version` becomes 2.
  - `DownloadRecord.auto_place: bool` (last field; `new_file` sets `false`).
  - `Store::insert_download_with_headers(DownloadRecord, Vec<(String, String)>)`: one transaction.
  - `Store::list_with_bytes() -> Vec<(DownloadRecord, u64)>` (newest first) and `Store::get_with_bytes(&str) -> Option<(DownloadRecord, u64)>`. The `u64` is the sum of that download's `chunks.written`.
  - `Store::set_placement(&str, category: String, save_dir: String)`.

- [ ] **Step 1: Write the failing tests** (append to `tests/store.rs`)

```rust
#[tokio::test]
async fn newer_schema_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("odm.db");
    {
        let s = Store::open(&path).unwrap();
        s.call(|c| c.pragma_update(None, "user_version", 99)).await.unwrap();
    }
    match Store::open(&path) {
        Err(odm_store::StoreError::NewerSchema { found, supported }) => {
            assert_eq!(found, 99);
            assert_eq!(supported, 2);
        }
        Err(e) => panic!("wrong error: {e}"),
        Ok(_) => panic!("opened a newer schema"),
    }
}

#[tokio::test]
async fn v1_database_is_migrated_to_v2() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("odm.db");
    {
        let s = Store::open(&path).unwrap();
        s.insert_download(rec("a")).await.unwrap();
        // Rewind to a V1-shaped database: drop the V2 column and the version.
        s.call(|c| {
            c.execute_batch("ALTER TABLE downloads DROP COLUMN auto_place;")?;
            c.pragma_update(None, "user_version", 1)
        })
        .await
        .unwrap();
    }
    let s = Store::open(&path).unwrap();
    let got = s.get_download("a").await.unwrap().unwrap();
    assert!(!got.auto_place);
    let v: i64 = s.call(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0))).await.unwrap();
    assert_eq!(v, 2);
}

#[tokio::test]
async fn insert_with_headers_is_atomic() {
    let s = Store::open_in_memory().unwrap();
    let mut r = rec("a");
    r.auto_place = true;
    s.insert_download_with_headers(r, vec![("Cookie".into(), "x=1".into())]).await.unwrap();
    assert!(s.get_download("a").await.unwrap().unwrap().auto_place);
    assert_eq!(s.get_headers("a").await.unwrap().len(), 1);

    // Second insert with the same id fails as a whole: no extra header rows appear.
    let err = s
        .insert_download_with_headers(rec("a"), vec![("Referer".into(), "r".into())])
        .await;
    assert!(err.is_err());
    assert_eq!(s.get_headers("a").await.unwrap().len(), 1);
}

#[tokio::test]
async fn bytes_come_from_chunks() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    s.insert_download(rec("b")).await.unwrap();
    s.replace_chunks(
        "a",
        vec![
            ChunkRecord { idx: 0, start: 0, end: Some(99), written: 40 },
            ChunkRecord { idx: 1, start: 100, end: Some(199), written: 2 },
        ],
    )
    .await
    .unwrap();
    let (ra, bytes_a) = s.get_with_bytes("a").await.unwrap().unwrap();
    assert_eq!((ra.id.as_str(), bytes_a), ("a", 42));
    assert_eq!(s.get_with_bytes("b").await.unwrap().unwrap().1, 0);
    assert!(s.get_with_bytes("nope").await.unwrap().is_none());
    let all = s.list_with_bytes().await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all.iter().find(|(r, _)| r.id == "a").unwrap().1, 42);
}

#[tokio::test]
async fn set_placement_updates_category_and_dir() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    s.set_placement("a", "video".into(), "C:/dl/Videos".into()).await.unwrap();
    let r = s.get_download("a").await.unwrap().unwrap();
    assert_eq!((r.category.as_str(), r.save_dir.as_str()), ("video", "C:/dl/Videos"));
}
```

Also change the existing assertion in `fresh_db_is_migrated_and_reopen_is_idempotent` from `assert_eq!(v, 1);` to `assert_eq!(v, 2);`. This is a deliberate update: the schema version really changed.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-store`
Expected: compile errors (`NewerSchema`, `auto_place`, `insert_download_with_headers`, `get_with_bytes` don't exist).

- [ ] **Step 3: Implement**

`migrations.rs`: add V2, return `StoreError`, and reject newer versions.
```rust
use crate::StoreError;

const V2: &str = "ALTER TABLE downloads ADD COLUMN auto_place INTEGER NOT NULL DEFAULT 0;";

/// Ordered; index i migrates to user_version i+1. Append only — never edit a shipped entry.
const MIGRATIONS: &[&str] = &[V1, V2];

pub fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let supported = MIGRATIONS.len() as i64;
    if current > supported || current < 0 {
        return Err(StoreError::NewerSchema { found: current, supported });
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}
```

`lib.rs`: add the error variant and the new methods.
```rust
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("store thread closed")]
    Closed,
    #[error("database schema {found} is newer than this app supports ({supported})")]
    NewerSchema { found: i64, supported: i64 },
}
```
```rust
    pub async fn insert_download_with_headers(
        &self,
        d: DownloadRecord,
        h: Vec<(String, String)>,
    ) -> Result<(), StoreError> {
        self.call(move |c| repo::insert_download_with_headers(c, &d, &h)).await
    }
    pub async fn list_with_bytes(&self) -> Result<Vec<(DownloadRecord, u64)>, StoreError> {
        self.call(|c| repo::list_with_bytes(c)).await
    }
    pub async fn get_with_bytes(&self, id: &str) -> Result<Option<(DownloadRecord, u64)>, StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::get_with_bytes(c, &id)).await
    }
    pub async fn set_placement(&self, id: &str, category: String, save_dir: String) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::set_placement(c, &id, &category, &save_dir)).await
    }
```

`models.rs`: add `pub auto_place: bool,` as the last field of `DownloadRecord`, and `auto_place: false,` in `new_file`.

`repo.rs`:
- Append `,auto_place` to `COLS` (25 columns). In `to_record` add `auto_place: r.get::<_, i64>(24)? != 0,`. In `insert_download` add `?25` to the VALUES list and `d.auto_place as i64` to the params.
- Add:
```rust
const BYTES: &str = "COALESCE((SELECT SUM(written) FROM chunks WHERE download_id = downloads.id), 0)";

pub fn insert_download_with_headers(
    c: &mut Connection,
    d: &DownloadRecord,
    h: &[(String, String)],
) -> rusqlite::Result<()> {
    let tx = c.transaction()?;
    insert_download(&tx, d)?;
    for (n, v) in h {
        tx.execute(
            "INSERT OR REPLACE INTO download_headers (download_id,name,value) VALUES (?1,?2,?3)",
            params![d.id, n, v],
        )?;
    }
    tx.commit()
}

pub fn list_with_bytes(c: &Connection) -> rusqlite::Result<Vec<(DownloadRecord, u64)>> {
    let mut st = c.prepare(&format!(
        "SELECT {COLS}, {BYTES} FROM downloads ORDER BY created_at DESC, rowid DESC"
    ))?;
    let rows = st.query_map([], |r| Ok((to_record(r)?, r.get::<_, i64>(25)? as u64)))?;
    rows.collect()
}

pub fn get_with_bytes(c: &Connection, id: &str) -> rusqlite::Result<Option<(DownloadRecord, u64)>> {
    c.query_row(
        &format!("SELECT {COLS}, {BYTES} FROM downloads WHERE id=?1"),
        [id],
        |r| Ok((to_record(r)?, r.get::<_, i64>(25)? as u64)),
    )
    .optional()
}

pub fn set_placement(c: &Connection, id: &str, category: &str, save_dir: &str) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE downloads SET category=?2, save_dir=?3 WHERE id=?1",
        params![id, category, save_dir],
    )?;
    Ok(())
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p odm-store`
Expected: 13 passed.

If `v1_database_is_migrated_to_v2` fails on `DROP COLUMN`: the bundled SQLite supports it (3.35+). Check the column name is exactly `auto_place`.

- [ ] **Step 5: Whole workspace still builds and passes**

Run: `cargo test --workspace -- --test-threads=4`
Expected: all pass. `DownloadRecord::new_file` callers are unaffected, because the new field has a default there.

- [ ] **Step 6: Commit**

```bash
git add crates/odm-store
git commit -m "feat(store): reject newer schema, auto_place column, atomic add, byte counts"
```

---

### Task 2: Engine additions (runtime config, read-only queries, MIME placement, limiter wake-up)

**Files:**
- Modify: `crates/odm-engine/src/config.rs`, `src/manager.rs`, `src/download.rs`, `src/limiter.rs`
- Modify: `crates/odm-testserver/src/lib.rs` (new fault `content_type`)
- Test: `crates/odm-engine/tests/manager.rs`, `tests/limiter.rs`, inline tests in `config.rs`

**Interfaces:**
- Consumes: Task 1's `insert_download_with_headers`, `list_with_bytes`, `get_with_bytes`, `set_placement`, `DownloadRecord.auto_place`.
- Produces:
  - `EngineConfig::sanitized(self) -> EngineConfig`: clamps `max_concurrent` to 1–10, `max_connections` to 1–32, `max_attempts` to 1–20, `connect_timeout` to 5–120 s, `read_timeout` to 10–300 s, `progress_interval` to at least 50 ms, `persist_interval` to at least 100 ms. `ManagerHandle::start` applies it.
  - `ManagerHandle::set_config(&self, EngineConfig) -> Result<(), OdmError>`: sanitizes, replaces the config and HTTP client used for downloads launched from now on, updates the speed limit and `max_concurrent`, then schedules.
  - `ManagerHandle::list(&self) -> Result<Vec<(DownloadRecord, u64)>, OdmError>`, `get(&self, id) -> Result<Option<(DownloadRecord, u64)>, OdmError>`, `chunks(&self, id) -> Result<Vec<ChunkRecord>, OdmError>`, `header_names(&self, id) -> Result<Vec<String>, OdmError>`. These read the store directly from the handle (no actor round trip).
  - `ManagerHandle::active_count(&self) -> Result<usize, OdmError>`: the number of downloads in probing, downloading or merging.
  - `manager.add` stores the record and headers in one transaction and sets `auto_place = req.category.is_none() && req.save_dir.is_none()`.
  - After a probe, a download with `auto_place` whose detected category (by extension, then MIME) differs from the stored one is moved: `set_placement(id, new_category, config.save_dir_for(new_category))`.
  - `SpeedLimiter::set_rate` wakes every caller currently sleeping in `acquire`.
  - Test server: `Faults.content_type: Option<String>` (default `None` → `application/octet-stream`).

- [ ] **Step 1: Write the failing tests**

Inline in `config.rs` tests module:
```rust
    #[test]
    fn sanitized_clamps_out_of_range_values() {
        use std::time::Duration;
        let c = super::EngineConfig {
            max_concurrent: 0,
            max_connections: 500,
            max_attempts: 0,
            connect_timeout: Duration::ZERO,
            read_timeout: Duration::from_secs(9999),
            progress_interval: Duration::ZERO,
            persist_interval: Duration::ZERO,
            ..Default::default()
        }
        .sanitized();
        assert_eq!((c.max_concurrent, c.max_connections, c.max_attempts), (1, 32, 1));
        assert_eq!(c.connect_timeout, Duration::from_secs(5));
        assert_eq!(c.read_timeout, Duration::from_secs(300));
        assert_eq!(c.progress_interval, Duration::from_millis(50));
        assert_eq!(c.persist_interval, Duration::from_millis(100));
        let d = super::EngineConfig::default();
        assert_eq!(d.clone().sanitized().max_concurrent, d.max_concurrent);
    }
```

Append to `tests/limiter.rs`:
```rust
#[tokio::test]
async fn raising_the_rate_wakes_sleepers() {
    let l = std::sync::Arc::new(SpeedLimiter::new(1_000));
    let l2 = l.clone();
    let t = Instant::now();
    let waiter = tokio::spawn(async move { l2.acquire(10_000).await }); // ~9 s at 1 KB/s
    tokio::time::sleep(Duration::from_millis(100)).await;
    l.set_rate(0);
    waiter.await.unwrap();
    assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
}
```

Append to `tests/manager.rs`:
```rust
#[tokio::test(flavor = "multi_thread")]
async fn set_config_applies_to_new_downloads() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = ManagerHandle::start(cfg(a.path()), Store::open_in_memory().unwrap()).await.unwrap();
    m.set_config(cfg(b.path())).await.unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed, 10).await;
    assert!(Path::new(&r.save_dir).starts_with(b.path()), "{}", r.save_dir);
    assert!(final_path(&r).exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn queries_report_bytes_and_active_count() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(2 * MB, Faults::default()).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap()).await.unwrap();
    assert_eq!(m.active_count().await.unwrap(), 0);
    let mut req = AddRequest::new(s.url("/file"));
    req.headers = vec![("Cookie".into(), "secret=1".into())];
    let id = m.add(req).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Completed, 20).await;
    let (rec, bytes) = m.get(&id).await.unwrap().unwrap();
    assert_eq!((rec.size, bytes), (Some(2 * MB), 2 * MB));
    assert_eq!(m.list().await.unwrap().len(), 1);
    assert!(!m.chunks(&id).await.unwrap().is_empty());
    assert_eq!(m.header_names(&id).await.unwrap(), vec!["Cookie".to_string()]);
    assert!(m.get("nope").await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn auto_placement_uses_the_probe_mime_type() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults { content_type: Some("video/mp4".into()), ..Default::default() }).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap()).await.unwrap();

    // No category and no folder given: placed by MIME type after the probe.
    let auto = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let r = wait_state(m.store(), &auto, DownloadState::Completed, 10).await;
    assert_eq!(r.category, "video");
    assert_eq!(Path::new(&r.save_dir), d.path().join("Videos"));
    assert!(final_path(&r).exists());

    // A folder chosen by the user is never changed.
    let mut req = AddRequest::new(s.url("/file"));
    req.save_dir = Some(d.path().join("mine"));
    let fixed = m.add(req).await.unwrap();
    let r = wait_state(m.store(), &fixed, DownloadState::Completed, 10).await;
    assert_eq!(Path::new(&r.save_dir), d.path().join("mine"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-engine`
Expected: compile errors (`sanitized`, `set_config`, `get`, `list`, `chunks`, `header_names`, `active_count`, `content_type` don't exist).

- [ ] **Step 3: Implement the test server fault**

In `crates/odm-testserver/src/lib.rs`: add `pub content_type: Option<String>,` to `Faults`, and in the `/file` handler replace the fixed Content-Type header with:
```rust
        .header(
            header::CONTENT_TYPE,
            f.content_type.as_deref().unwrap_or("application/octet-stream"),
        );
```

- [ ] **Step 4: Implement `EngineConfig::sanitized`** (in `config.rs`, inside `impl EngineConfig`)

```rust
    /// Clamp every field to its supported range (spec section 5 and 9).
    pub fn sanitized(mut self) -> Self {
        self.max_concurrent = self.max_concurrent.clamp(1, 10);
        self.max_connections = self.max_connections.clamp(1, 32);
        self.max_attempts = self.max_attempts.clamp(1, 20);
        self.connect_timeout = self.connect_timeout.clamp(Duration::from_secs(5), Duration::from_secs(120));
        self.read_timeout = self.read_timeout.clamp(Duration::from_secs(10), Duration::from_secs(300));
        self.progress_interval = self.progress_interval.max(Duration::from_millis(50));
        self.persist_interval = self.persist_interval.max(Duration::from_millis(100));
        self
    }
```

- [ ] **Step 5: Implement the limiter wake-up** (in `limiter.rs`)

Add a `changed: tokio::sync::Notify` field next to `inner` (initialise with `Notify::new()` in `new`). Then:
```rust
    pub fn set_rate(&self, bps: u64) {
        {
            let mut b = self.inner.lock().unwrap();
            b.rate = bps;
            b.tokens = bps as f64;
            b.last = Instant::now();
        }
        // Anyone sleeping off debt under the old rate re-evaluates now.
        self.changed.notify_waiters();
    }

    pub async fn acquire(&self, n: usize) {
        // Created before the wait is computed, so a set_rate in between is not missed.
        let changed = self.changed.notified();
        tokio::pin!(changed);
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
        };
        if !wait.is_zero() {
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = &mut changed => {} // rate changed: the bucket was reset, old debt is void
            }
        }
    }
```

- [ ] **Step 6: Implement the manager changes** (in `manager.rs`)

- In `ManagerHandle::start`: `let config = config.sanitized();` as the first line.
- Add the command `SetConfig(Box<EngineConfig>)` to `Cmd`, and handle it in `run`:
```rust
                Cmd::SetConfig(cfg) => {
                    let cfg = cfg.sanitized();
                    self.env.limiter.set_rate(cfg.speed_limit_bps);
                    self.env.client = build_client(&cfg);
                    self.max_concurrent = cfg.max_concurrent;
                    self.env.config = cfg;
                    self.schedule().await;
                }
```
- Add to `impl ManagerHandle`:
```rust
    pub async fn set_config(&self, config: EngineConfig) -> Result<(), OdmError> {
        self.send(Cmd::SetConfig(Box::new(config))).await
    }

    pub async fn list(&self) -> Result<Vec<(DownloadRecord, u64)>, OdmError> {
        self.store.list_with_bytes().await.map_err(store_err)
    }

    pub async fn get(&self, id: &str) -> Result<Option<(DownloadRecord, u64)>, OdmError> {
        self.store.get_with_bytes(id).await.map_err(store_err)
    }

    pub async fn chunks(&self, id: &str) -> Result<Vec<odm_store::ChunkRecord>, OdmError> {
        self.store.get_chunks(id).await.map_err(store_err)
    }

    /// Header names only: values (cookies, tokens) never leave the engine.
    pub async fn header_names(&self, id: &str) -> Result<Vec<String>, OdmError> {
        Ok(self.store.get_headers(id).await.map_err(store_err)?.into_iter().map(|(n, _)| n).collect())
    }

    pub async fn active_count(&self) -> Result<usize, OdmError> {
        Ok(self.list().await?.iter().filter(|(r, _)| r.state.is_active()).count())
    }
```
  with a module-level helper:
```rust
fn store_err(e: odm_store::StoreError) -> OdmError {
    OdmError::new(ErrorKind::Other, format!("store: {e}"))
}
```
  (reuse it in `add` in place of the local closure).
- In `Manager::add`: set `rec.auto_place = req.category.is_none() && req.save_dir.is_none();` and replace the `insert_download` + `set_headers` pair with one call:
```rust
        self.env.store.insert_download_with_headers(rec, req.headers).await.map_err(store_err)?;
```

- [ ] **Step 7: Implement MIME placement** (in `download.rs`, in `run_inner`, right after the `rec = load(env, id).await?;` that follows `update_probe`)

```rust
            if rec.auto_place {
                let detected = odm_core::category::detect(&rec.filename, rec.mime.as_deref());
                if detected.as_str() != rec.category {
                    let dir = env.config.save_dir_for(detected.as_str());
                    env.store
                        .set_placement(id, detected.as_str().to_string(), dir.to_string_lossy().into_owned())
                        .await
                        .map_err(store_err)?;
                    rec = load(env, id).await?;
                }
            }
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test -p odm-engine` (then run `--test manager` and `--test limiter` twice more: they are timing-based)
Expected: all pass.

- [ ] **Step 9: Lint and commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings`
```bash
git add crates/odm-engine crates/odm-testserver
git commit -m "feat(engine): runtime set_config, read-only queries, MIME placement, limiter wake-up"
```

---

### Task 3: `odm-appcore`: app paths and settings

**Files:**
- Modify: `Cargo.toml` (workspace deps: add `odm-appcore = { path = "crates/odm-appcore" }` and `tracing-appender = "0.2"`)
- Create: `crates/odm-appcore/Cargo.toml`, `src/lib.rs`, `src/paths.rs`, `src/settings.rs`
- Test: `crates/odm-appcore/tests/settings.rs`

**Interfaces:**
- Consumes: `odm_engine::EngineConfig` (with `sanitized()` from Task 2), `odm_core::ids::APP_DIR_NAME`, `odm_core::Category`.
- Produces:
  - `AppPaths { data_dir, db, settings, logs }` (all `PathBuf`), `AppPaths::at(dir)`, `AppPaths::default_location()` (`%APPDATA%\ODM`), `AppPaths::ensure(&self) -> std::io::Result<()>` (creates `data_dir` and `logs`).
  - `Settings` (serde, `PartialEq`, `Clone`) with fields: `download_dir: String`, `category_folders: BTreeMap<String, String>`, `max_concurrent: u32`, `max_connections: u32`, `max_attempts: u32`, `speed_limit_kbps: u64`, `connect_timeout_s: u64`, `read_timeout_s: u64`, `ffmpeg_path: String`, `capture_enabled: bool`, `min_capture_bytes: u64`, `skip_hosts: Vec<String>`, `close_to_tray: bool`, `start_minimized: bool`, `notifications: bool`.
  - `Settings::from_json(&str) -> (Settings, Vec<String>)`: tolerant parse; the `Vec` holds one warning per problem.
  - `Settings::load(&Path) -> (Settings, Vec<String>)`: a missing file gives defaults and no warnings.
  - `Settings::clamp(&mut self) -> Vec<String>`, `Settings::save(&self, &Path) -> std::io::Result<()>` (atomic), `Settings::to_engine_config(&self) -> EngineConfig`.

- [ ] **Step 1: Manifest and lib**

`crates/odm-appcore/Cargo.toml`:
```toml
[package]
name = "odm-appcore"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
odm-core.workspace = true
odm-store.workspace = true
odm-engine.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
tracing.workspace = true
tracing-appender.workspace = true
tracing-subscriber.workspace = true

[dev-dependencies]
odm-testserver.workspace = true
tempfile.workspace = true
```

`src/lib.rs`:
```rust
//! Everything the desktop app needs that does not depend on Tauri.
pub mod paths;
pub mod settings;

pub use paths::AppPaths;
pub use settings::Settings;
```

- [ ] **Step 2: Write the failing tests**

`crates/odm-appcore/tests/settings.rs`:
```rust
use odm_appcore::{AppPaths, Settings};
use std::time::Duration;

#[test]
fn defaults_match_the_spec() {
    let s = Settings::default();
    assert_eq!((s.max_concurrent, s.max_connections, s.max_attempts), (3, 8, 5));
    assert_eq!((s.connect_timeout_s, s.read_timeout_s, s.speed_limit_kbps), (30, 60, 0));
    assert_eq!(s.min_capture_bytes, 1 << 20);
    assert!(s.capture_enabled && s.close_to_tray && s.notifications && !s.start_minimized);
    assert_eq!(s.category_folders["video"], "Videos");
    assert_eq!(s.category_folders["general"], "");
    assert!(s.download_dir.ends_with("Downloads"));
}

#[test]
fn missing_file_gives_defaults_without_warnings() {
    let d = tempfile::tempdir().unwrap();
    let (s, warnings) = Settings::load(&d.path().join("settings.json"));
    assert_eq!(s, Settings::default());
    assert!(warnings.is_empty());
}

#[test]
fn corrupt_file_falls_back_to_defaults() {
    for text in ["{not json", "", "[1,2,3]", r#"{"max_concurrent": 5, "close_to"#] {
        let (s, warnings) = Settings::from_json(text);
        assert_eq!(s, Settings::default(), "input: {text:?}");
        assert_eq!(warnings.len(), 1, "input: {text:?}");
    }
}

#[test]
fn one_bad_field_keeps_the_rest() {
    let (s, warnings) = Settings::from_json(
        r#"{"max_concurrent": "many", "max_connections": 16, "close_to_tray": false}"#,
    );
    assert_eq!(s.max_concurrent, 3); // default kept
    assert_eq!(s.max_connections, 16);
    assert!(!s.close_to_tray);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("max_concurrent"), "{warnings:?}");
}

#[test]
fn unknown_fields_are_reported_and_ignored() {
    let (s, warnings) = Settings::from_json(r#"{"theme": "light", "max_attempts": 7}"#);
    assert_eq!(s.max_attempts, 7);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("theme"));
}

#[test]
fn out_of_range_values_are_clamped() {
    let (s, warnings) = Settings::from_json(
        r#"{"max_concurrent": 99, "max_connections": 0, "max_attempts": 0,
            "connect_timeout_s": 1, "read_timeout_s": 100000, "download_dir": "  "}"#,
    );
    assert_eq!((s.max_concurrent, s.max_connections, s.max_attempts), (10, 1, 1));
    assert_eq!((s.connect_timeout_s, s.read_timeout_s), (5, 300));
    assert_eq!(s.download_dir, Settings::default().download_dir);
    assert_eq!(warnings.len(), 6, "{warnings:?}");
}

#[test]
fn unsafe_category_folders_are_reset() {
    let (s, warnings) = Settings::from_json(
        r#"{"category_folders": {"video": "..\\..\\Windows", "audio": "C:\\x", "image": "Pics", "document": "a/b"}}"#,
    );
    assert_eq!(s.category_folders["video"], "Videos");
    assert_eq!(s.category_folders["audio"], "Music");
    assert_eq!(s.category_folders["document"], "Documents");
    assert_eq!(s.category_folders["image"], "Pics");
    // Categories missing from the file keep their defaults.
    assert_eq!(s.category_folders["compressed"], "Compressed");
    assert_eq!(warnings.len(), 3, "{warnings:?}");
}

#[test]
fn save_is_atomic_and_round_trips() {
    let d = tempfile::tempdir().unwrap();
    let p = AppPaths::at(d.path());
    p.ensure().unwrap();
    let mut s = Settings::default();
    s.max_connections = 12;
    s.skip_hosts = vec!["example.com".into()];
    s.save(&p.settings).unwrap();
    let (back, warnings) = Settings::load(&p.settings);
    assert_eq!(back, s);
    assert!(warnings.is_empty());
    let leftovers: Vec<_> = std::fs::read_dir(d.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn to_engine_config_maps_units() {
    let mut s = Settings::default();
    s.speed_limit_kbps = 500;
    s.max_concurrent = 4;
    s.connect_timeout_s = 10;
    s.download_dir = "D:/dl".into();
    let c = s.to_engine_config();
    assert_eq!(c.speed_limit_bps, 500 * 1024);
    assert_eq!(c.max_concurrent, 4);
    assert_eq!(c.connect_timeout, Duration::from_secs(10));
    assert_eq!(c.download_dir, std::path::PathBuf::from("D:/dl"));
    assert_eq!(c.category_folders["video"], "Videos");
}

#[test]
fn paths_layout() {
    let p = AppPaths::at("X:/data");
    assert!(p.db.ends_with("odm.db") && p.settings.ends_with("settings.json") && p.logs.ends_with("logs"));
    assert!(AppPaths::default_location().data_dir.ends_with("ODM"));
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p odm-appcore`
Expected: compile errors (the modules are empty).

- [ ] **Step 4: Implement `src/paths.rs`**

```rust
use std::path::PathBuf;

/// Where the app keeps its files. `%APPDATA%\ODM` in production.
#[derive(Debug, Clone, PartialEq)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub db: PathBuf,
    pub settings: PathBuf,
    pub logs: PathBuf,
}

impl AppPaths {
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        let data_dir = dir.into();
        Self {
            db: data_dir.join("odm.db"),
            settings: data_dir.join("settings.json"),
            logs: data_dir.join("logs"),
            data_dir,
        }
    }

    pub fn default_location() -> Self {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        Self::at(base.join(odm_core::ids::APP_DIR_NAME))
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.logs)
    }
}
```

- [ ] **Step 5: Implement `src/settings.rs`**

```rust
use odm_core::Category;
use odm_engine::EngineConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub download_dir: String,
    pub category_folders: BTreeMap<String, String>,
    pub max_concurrent: u32,
    pub max_connections: u32,
    pub max_attempts: u32,
    /// KB/s; 0 = unlimited.
    pub speed_limit_kbps: u64,
    pub connect_timeout_s: u64,
    pub read_timeout_s: u64,
    pub ffmpeg_path: String,
    pub capture_enabled: bool,
    pub min_capture_bytes: u64,
    pub skip_hosts: Vec<String>,
    pub close_to_tray: bool,
    pub start_minimized: bool,
    pub notifications: bool,
}

fn default_folders() -> BTreeMap<String, String> {
    Category::ALL.iter().map(|c| (c.as_str().to_string(), c.default_folder().to_string())).collect()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: EngineConfig::default().download_dir.to_string_lossy().into_owned(),
            category_folders: default_folders(),
            max_concurrent: 3,
            max_connections: 8,
            max_attempts: 5,
            speed_limit_kbps: 0,
            connect_timeout_s: 30,
            read_timeout_s: 60,
            ffmpeg_path: String::new(),
            capture_enabled: true,
            min_capture_bytes: 1 << 20,
            skip_hosts: Vec::new(),
            close_to_tray: true,
            start_minimized: false,
            notifications: true,
        }
    }
}

fn clamp_num<T: PartialOrd + Copy + std::fmt::Display>(name: &str, v: &mut T, lo: T, hi: T, w: &mut Vec<String>) {
    if *v < lo || *v > hi {
        let fixed = if *v < lo { lo } else { hi };
        w.push(format!("{name}: {v} is outside {lo}..{hi}; using {fixed}"));
        *v = fixed;
    }
}

/// A category folder must be a single plain folder name under the download directory.
fn safe_folder(name: &str) -> bool {
    !name.contains(['\\', '/', ':']) && name != ".." && name != "." && name.trim() == name
}

impl Settings {
    /// Parse leniently: each bad or unknown field is reported and the default kept.
    pub fn from_json(text: &str) -> (Settings, Vec<String>) {
        let mut warnings = Vec::new();
        let obj = match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(o)) => o,
            Ok(_) => {
                return (Settings::default(), vec!["settings file is not a JSON object; using defaults".into()]);
            }
            Err(e) => return (Settings::default(), vec![format!("settings file is not valid JSON ({e}); using defaults")]),
        };
        let mut base = serde_json::to_value(Settings::default()).expect("settings serialize");
        for (key, value) in obj {
            if base.get(&key).is_none() {
                warnings.push(format!("{key}: unknown setting ignored"));
                continue;
            }
            let mut trial = base.clone();
            trial[&key] = value;
            if serde_json::from_value::<Settings>(trial.clone()).is_ok() {
                base = trial;
            } else {
                warnings.push(format!("{key}: invalid value; using the default"));
            }
        }
        let mut s: Settings = serde_json::from_value(base).expect("validated above");
        warnings.extend(s.clamp());
        (s, warnings)
    }

    pub fn clamp(&mut self) -> Vec<String> {
        let mut w = Vec::new();
        clamp_num("max_concurrent", &mut self.max_concurrent, 1, 10, &mut w);
        clamp_num("max_connections", &mut self.max_connections, 1, 32, &mut w);
        clamp_num("max_attempts", &mut self.max_attempts, 1, 20, &mut w);
        clamp_num("connect_timeout_s", &mut self.connect_timeout_s, 5, 120, &mut w);
        clamp_num("read_timeout_s", &mut self.read_timeout_s, 10, 300, &mut w);
        if self.download_dir.trim().is_empty() {
            w.push("download_dir: empty; using the default".into());
            self.download_dir = Settings::default().download_dir;
        }
        let defaults = default_folders();
        for (cat, def) in &defaults {
            match self.category_folders.get(cat) {
                Some(f) if safe_folder(f) => {}
                Some(f) => {
                    w.push(format!("category_folders.{cat}: {f:?} is not a plain folder name; using {def:?}"));
                    self.category_folders.insert(cat.clone(), def.clone());
                }
                None => {
                    self.category_folders.insert(cat.clone(), def.clone());
                }
            }
        }
        self.category_folders.retain(|k, _| defaults.contains_key(k));
        w
    }

    pub fn load(path: &Path) -> (Settings, Vec<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => Settings::from_json(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Settings::default(), Vec::new()),
            Err(e) => (Settings::default(), vec![format!("cannot read settings ({e}); using defaults")]),
        }
    }

    /// Write to a temp file, then rename over the real one.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).expect("settings serialize"))?;
        std::fs::rename(&tmp, path)
    }

    pub fn to_engine_config(&self) -> EngineConfig {
        EngineConfig {
            download_dir: PathBuf::from(&self.download_dir),
            category_folders: self.category_folders.clone().into_iter().collect(),
            max_concurrent: self.max_concurrent as usize,
            max_connections: self.max_connections,
            max_attempts: self.max_attempts,
            connect_timeout: Duration::from_secs(self.connect_timeout_s),
            read_timeout: Duration::from_secs(self.read_timeout_s),
            speed_limit_bps: self.speed_limit_kbps.saturating_mul(1024),
            ..EngineConfig::default()
        }
        .sanitized()
    }
}
```

Notes for the tests' warning counts:
- `corrupt_file_falls_back_to_defaults`: the empty string and the truncated text are invalid JSON; `[1,2,3]` is valid JSON but not an object. Each yields exactly one warning.
- `out_of_range_values_are_clamped` expects 6 warnings: five numeric clamps and the blank `download_dir`.
- `unsafe_category_folders_are_reset` expects 3: `video`, `audio` and `document`.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p odm-appcore`
Expected: 10 passed.

- [ ] **Step 7: Lint and commit**

Run: `cargo fmt --all && cargo clippy -p odm-appcore --all-targets -- -D warnings`
```bash
git add Cargo.toml Cargo.lock crates/odm-appcore
git commit -m "feat(appcore): app paths and tolerant, atomic settings"
```

---

### Task 4: Logging and URL redaction

**Files:**
- Create: `crates/odm-core/src/redact.rs`; modify `crates/odm-core/src/lib.rs` (add `pub mod redact; pub use redact::redact_url;`)
- Modify: `crates/odm-engine/src/download.rs` (use `odm_core::redact_url` wherever `log_url` is used; delete `log_url`)
- Create: `crates/odm-appcore/src/logging.rs`; modify `crates/odm-appcore/src/lib.rs` (add `pub mod logging;`)

**Interfaces:**
- Produces:
  - `odm_core::redact_url(url: &str) -> String`: removes the query string, the fragment and any `user:pass@` part.
  - `odm_appcore::logging::file_writer(dir: &Path) -> std::io::Result<(tracing_appender::non_blocking::NonBlocking, tracing_appender::non_blocking::WorkerGuard)>`: daily rotation, files named `odm.<date>.log`, 7 kept.
  - `odm_appcore::logging::init(dir: &Path) -> std::io::Result<WorkerGuard>`: installs the global subscriber (level from `RUST_LOG`, default `info`; no ANSI colours). The caller keeps the guard alive for the life of the app.

- [ ] **Step 1: Write the failing tests**

Inline in `crates/odm-core/src/redact.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::redact_url;

    #[test]
    fn strips_query_fragment_and_userinfo() {
        assert_eq!(redact_url("https://a.com/f.zip?token=secret#x"), "https://a.com/f.zip");
        assert_eq!(redact_url("https://user:pw@a.com/f.zip"), "https://a.com/f.zip");
        assert_eq!(redact_url("https://user@a.com:8443/p/q?x=1"), "https://a.com:8443/p/q");
        assert_eq!(redact_url("http://a.com"), "http://a.com");
    }

    #[test]
    fn an_at_sign_in_the_path_is_not_userinfo() {
        assert_eq!(redact_url("https://a.com/files/me@home.txt"), "https://a.com/files/me@home.txt");
    }

    #[test]
    fn non_urls_lose_only_query_and_fragment() {
        assert_eq!(redact_url("not a url?x=1"), "not a url");
    }
}
```

Inline in `crates/odm-appcore/src/logging.rs`:
```rust
#[cfg(test)]
mod tests {
    use std::io::Write;

    #[test]
    fn file_writer_creates_a_dated_log_file() {
        let d = tempfile::tempdir().unwrap();
        {
            let (mut w, _guard) = super::file_writer(d.path()).unwrap();
            w.write_all(b"hello\n").unwrap();
        } // guard dropped: the background writer flushes
        let names: Vec<String> = std::fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
        assert!(names[0].starts_with("odm.") && names[0].ends_with(".log"), "{names:?}");
        let text = std::fs::read_to_string(d.path().join(&names[0])).unwrap();
        assert_eq!(text, "hello\n");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-core redact && cargo test -p odm-appcore --lib logging`
Expected: compile errors.

- [ ] **Step 3: Implement `crates/odm-core/src/redact.rs`** (above the tests)

```rust
//! Make a URL safe to write to a log.

/// Remove the query string, the fragment, and any `user:pass@` credentials.
pub fn redact_url(url: &str) -> String {
    let no_tail = url.split(['?', '#']).next().unwrap_or(url);
    let Some((scheme, rest)) = no_tail.split_once("://") else {
        return no_tail.to_string();
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    format!("{scheme}://{host}{path}")
}
```

- [ ] **Step 4: Use it in the engine**

In `crates/odm-engine/src/download.rs`, delete `pub fn log_url` and replace each call `log_url(x)` with `odm_core::redact_url(x)` (the logging macro takes the `String` by value, for example `url = %odm_core::redact_url(&rec.url)`). Check with `grep -rn "log_url" crates/` that nothing else refers to it; update any test that imported it.

- [ ] **Step 5: Implement `crates/odm-appcore/src/logging.rs`** (above the tests)

```rust
use std::path::Path;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;

const KEEP_FILES: usize = 7;

/// A non-blocking writer onto `dir/odm.<date>.log`, rotated daily.
pub fn file_writer(dir: &Path) -> std::io::Result<(NonBlocking, WorkerGuard)> {
    std::fs::create_dir_all(dir)?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("odm")
        .filename_suffix("log")
        .max_log_files(KEEP_FILES)
        .build(dir)
        .map_err(std::io::Error::other)?;
    Ok(tracing_appender::non_blocking(appender))
}

/// Install the global logger. Keep the returned guard alive until the app exits.
pub fn init(dir: &Path) -> std::io::Result<WorkerGuard> {
    let (writer, guard) = file_writer(dir)?;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init()
        .map_err(std::io::Error::other)?;
    Ok(guard)
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p odm-core && cargo test -p odm-appcore && cargo test -p odm-engine --test download`
Expected: all pass.

- [ ] **Step 7: Lint and commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings`
```bash
git add crates/odm-core crates/odm-engine crates/odm-appcore
git commit -m "feat: rotating log files and credential-free URL logging"
```

---

### Task 5: `odm-appcore`: DTOs and the functions behind each command

**Files:**
- Create: `crates/odm-appcore/src/dto.rs`, `src/api.rs`; modify `src/lib.rs` (add `pub mod api; pub mod dto;`)
- Test: `crates/odm-appcore/tests/api.rs`

**Interfaces:**
- Consumes: `ManagerHandle::{add, list, get, chunks, header_names, set_config}`, `AddRequest`, `odm_engine::{probe, ChunkView, ChunkViewState}`, `odm_engine::download::build_client`, `odm_core::{filename, category}`.
- Produces (all `Serialize`; JSON field names are snake_case and are the UI contract):
  - `DownloadDto { id, url, filename, save_dir, size: Option<u64>, bytes: u64, mime: Option<String>, category: String, state: DownloadState, error_code: Option<String>, error_message: Option<String>, max_connections: u32, kind: String, page_url: Option<String>, page_title: Option<String>, created_at: i64, started_at: Option<i64>, finished_at: Option<i64> }`
  - `dto::to_dto(&DownloadRecord, bytes: u64) -> DownloadDto`. For a completed download with a known size, `bytes` is the size.
  - `DetailDto { download: DownloadDto, final_url: Option<String>, chunks: Vec<ChunkView>, header_names: Vec<String> }`
  - `UiEvent` (`#[serde(tag = "type", rename_all = "snake_case")]`): `Upsert { download: DownloadDto }`, `Removed { id: String }`, `ProgressBatch { items: Vec<ProgressSnapshot> }`, `Resync`.
  - `api::AddArgs` (`Deserialize`, `Default`): `url: String`, `filename: Option<String>`, `save_dir: Option<String>`, `connections: Option<u32>`, `category: Option<String>`, `checksum: Option<String>`, `start_paused: bool` (missing = false). Blank strings count as not given.
  - `api::list(&ManagerHandle) -> Result<Vec<DownloadDto>, String>`
  - `api::detail(&ManagerHandle, id: &str) -> Result<Option<DetailDto>, String>`
  - `api::add(&ManagerHandle, AddArgs) -> Result<String, String>`
  - `api::parse_urls(text: &str) -> ParsedUrls { valid: Vec<String>, rejected: Vec<String> }`
  - `api::add_batch(&ManagerHandle, text: &str, save_dir: Option<String>) -> Result<BatchResult, String>` where `BatchResult { added: Vec<String> /*ids*/, rejected: Vec<String> }`
  - `api::suggest(url: &str, &EngineConfig) -> Result<Suggestion, String>` where `Suggestion { filename: String, size: Option<u64>, category: String, resumable: bool }`
  - `api::apply_settings(&ManagerHandle, &Settings) -> Result<(), String>`

- [ ] **Step 1: Write the failing tests**

`crates/odm-appcore/tests/api.rs`:
```rust
use odm_appcore::api::{self, AddArgs};
use odm_appcore::dto::UiEvent;
use odm_appcore::Settings;
use odm_core::DownloadState;
use odm_engine::{ChunkViewState, EngineConfig, ManagerHandle};
use odm_store::Store;
use odm_testserver::{Faults, TestServer};
use std::path::Path;
use std::time::{Duration, Instant};

fn cfg(dir: &Path) -> EngineConfig {
    EngineConfig {
        download_dir: dir.to_path_buf(),
        progress_interval: Duration::from_millis(50),
        persist_interval: Duration::from_millis(200),
        ..Default::default()
    }
}

async fn manager(dir: &Path) -> ManagerHandle {
    ManagerHandle::start(cfg(dir), Store::open_in_memory().unwrap()).await.unwrap()
}

async fn wait_completed(m: &ManagerHandle, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let (r, _) = m.get(id).await.unwrap().unwrap();
        if r.state == DownloadState::Completed {
            return;
        }
        assert!(r.state != DownloadState::Failed, "{:?}", r.error_msg);
        assert!(Instant::now() < deadline, "timed out in state {:?}", r.state);
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[test]
fn parse_urls_filters_and_dedupes() {
    let text = "https://a.com/1.zip\n\n  http://b.com/2.zip  \nftp://c.com/3\nhttps://a.com/1.zip\nnot a url\nHTTPS://D.com/4\nhttps://\n";
    let p = api::parse_urls(text);
    assert_eq!(p.valid, vec!["https://a.com/1.zip", "http://b.com/2.zip", "HTTPS://D.com/4"]);
    assert_eq!(p.rejected, vec!["ftp://c.com/3", "not a url", "https://"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn add_list_and_detail_round_trip() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(2 << 20, Faults::default()).await;
    let m = manager(d.path()).await;
    let id = api::add(
        &m,
        AddArgs { url: s.url("/file"), filename: Some("  ".into()), category: Some(String::new()), ..Default::default() },
    )
    .await
    .unwrap();
    wait_completed(&m, &id).await;

    let list = api::list(&m).await.unwrap();
    assert_eq!(list.len(), 1);
    let dto = &list[0];
    assert_eq!(dto.filename, "file"); // blank filename was treated as "not given"
    assert_eq!((dto.size, dto.bytes), (Some(2 << 20), 2 << 20));
    assert_eq!(dto.state, DownloadState::Completed);

    let detail = api::detail(&m, &id).await.unwrap().unwrap();
    assert!(!detail.chunks.is_empty());
    assert!(detail.chunks.iter().all(|c| c.state == ChunkViewState::Done));
    assert!(api::detail(&m, "nope").await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn detail_exposes_header_names_never_values() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = manager(d.path()).await;
    let mut req = odm_engine::AddRequest::new(s.url("/file"));
    req.headers = vec![("Cookie".into(), "session=TOPSECRET".into())];
    let id = m.add(req).await.unwrap();
    wait_completed(&m, &id).await;
    let detail = api::detail(&m, &id).await.unwrap().unwrap();
    assert_eq!(detail.header_names, vec!["Cookie".to_string()]);
    let json = serde_json::to_string(&detail).unwrap();
    assert!(!json.contains("TOPSECRET"), "{json}");
}

#[tokio::test(flavor = "multi_thread")]
async fn add_rejects_bad_urls_with_a_message() {
    let d = tempfile::tempdir().unwrap();
    let m = manager(d.path()).await;
    let err = api::add(&m, AddArgs { url: "ftp://x/y".into(), ..Default::default() }).await.unwrap_err();
    assert!(err.contains("http"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn batch_adds_valid_urls_and_reports_the_rest() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = manager(d.path()).await;
    let text = format!("{}\nnope\n{}\n", s.url("/file"), s.url("/file"));
    let r = api::add_batch(&m, &text, Some(d.path().join("batch").to_string_lossy().into_owned())).await.unwrap();
    assert_eq!(r.added.len(), 1);
    assert_eq!(r.rejected, vec!["nope".to_string()]);
    wait_completed(&m, &r.added[0]).await;
    assert!(d.path().join("batch").join("file").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn suggest_reads_name_size_and_category_from_the_server() {
    let s = TestServer::start(5000, Faults { filename: Some("report.pdf".into()), ..Default::default() }).await;
    let sug = api::suggest(&s.url("/file"), &EngineConfig::default()).await.unwrap();
    assert_eq!(sug.filename, "report.pdf");
    assert_eq!(sug.size, Some(5000));
    assert_eq!(sug.category, "document");
    assert!(sug.resumable);

    let s = TestServer::start(10, Faults { fail_status: Some(404), fail_times: 9, ..Default::default() }).await;
    assert!(api::suggest(&s.url("/file"), &EngineConfig::default()).await.unwrap_err().contains("http_404"));
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_settings_changes_where_new_downloads_go() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = manager(a.path()).await;
    let settings = Settings { download_dir: b.path().to_string_lossy().into_owned(), ..Default::default() };
    api::apply_settings(&m, &settings).await.unwrap();
    let id = api::add(&m, AddArgs { url: s.url("/file"), ..Default::default() }).await.unwrap();
    wait_completed(&m, &id).await;
    assert!(b.path().join("file").exists());
}

#[test]
fn ui_event_json_shape_is_pinned() {
    let json = serde_json::to_string(&UiEvent::Removed { id: "x".into() }).unwrap();
    assert_eq!(json, r#"{"type":"removed","id":"x"}"#);
    let json = serde_json::to_string(&UiEvent::Resync).unwrap();
    assert_eq!(json, r#"{"type":"resync"}"#);
    let json = serde_json::to_string(&UiEvent::ProgressBatch { items: vec![] }).unwrap();
    assert_eq!(json, r#"{"type":"progress_batch","items":[]}"#);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-appcore --test api`
Expected: compile errors (`api`, `dto` don't exist).

- [ ] **Step 3: Implement `src/dto.rs`**

```rust
use odm_core::DownloadState;
use odm_engine::{ChunkView, ChunkViewState, ProgressSnapshot};
use odm_store::{ChunkRecord, DownloadRecord};
use serde::Serialize;

/// One row of the download list, as the UI sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DownloadDto {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub save_dir: String,
    pub size: Option<u64>,
    pub bytes: u64,
    pub mime: Option<String>,
    pub category: String,
    pub state: DownloadState,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub max_connections: u32,
    pub kind: String,
    pub page_url: Option<String>,
    pub page_title: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

pub fn to_dto(r: &DownloadRecord, bytes: u64) -> DownloadDto {
    let bytes = match (r.state, r.size) {
        (DownloadState::Completed, Some(size)) => size,
        _ => bytes,
    };
    DownloadDto {
        id: r.id.clone(),
        url: r.url.clone(),
        filename: r.filename.clone(),
        save_dir: r.save_dir.clone(),
        size: r.size,
        bytes,
        mime: r.mime.clone(),
        category: r.category.clone(),
        state: r.state,
        error_code: r.error_kind.clone(),
        error_message: r.error_msg.clone(),
        max_connections: r.max_connections,
        kind: r.kind.clone(),
        page_url: r.page_url.clone(),
        page_title: r.page_title.clone(),
        created_at: r.created_at,
        started_at: r.started_at,
        finished_at: r.finished_at,
    }
}

/// Chunk views built from stored records, for a download that is not running.
pub fn chunk_views(chunks: &[ChunkRecord]) -> Vec<ChunkView> {
    chunks
        .iter()
        .map(|c| {
            let done = matches!(c.end, Some(e) if c.start + c.written > e);
            ChunkView {
                idx: c.idx,
                start: c.start,
                end: c.end,
                written: c.written,
                state: if done { ChunkViewState::Done } else { ChunkViewState::Pending },
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DetailDto {
    pub download: DownloadDto,
    pub final_url: Option<String>,
    pub chunks: Vec<ChunkView>,
    /// Names only. Header values (cookies, tokens) never leave the engine.
    pub header_names: Vec<String>,
}

/// Everything the UI receives on the `odm://event` channel.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    Upsert { download: DownloadDto },
    Removed { id: String },
    ProgressBatch { items: Vec<ProgressSnapshot> },
    /// The UI missed events and must reload the list.
    Resync,
}
```

- [ ] **Step 4: Implement `src/api.rs`**

```rust
use odm_core::{category, filename};
use odm_engine::download::build_client;
use odm_engine::{AddRequest, EngineConfig, ManagerHandle};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::dto::{chunk_views, to_dto, DetailDto, DownloadDto};
use crate::Settings;

fn msg(e: odm_core::OdmError) -> String {
    e.to_string()
}

fn given(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

pub async fn list(m: &ManagerHandle) -> Result<Vec<DownloadDto>, String> {
    Ok(m.list().await.map_err(msg)?.iter().map(|(r, b)| to_dto(r, *b)).collect())
}

pub async fn detail(m: &ManagerHandle, id: &str) -> Result<Option<DetailDto>, String> {
    let Some((rec, bytes)) = m.get(id).await.map_err(msg)? else {
        return Ok(None);
    };
    Ok(Some(DetailDto {
        download: to_dto(&rec, bytes),
        final_url: rec.final_url.clone(),
        chunks: chunk_views(&m.chunks(id).await.map_err(msg)?),
        header_names: m.header_names(id).await.map_err(msg)?,
    }))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AddArgs {
    pub url: String,
    pub filename: Option<String>,
    pub save_dir: Option<String>,
    pub connections: Option<u32>,
    pub category: Option<String>,
    pub checksum: Option<String>,
    pub start_paused: bool,
}

pub async fn add(m: &ManagerHandle, a: AddArgs) -> Result<String, String> {
    let mut req = AddRequest::new(a.url.trim());
    req.filename = given(a.filename);
    req.save_dir = given(a.save_dir).map(PathBuf::from);
    req.category = given(a.category);
    req.checksum = given(a.checksum);
    req.max_connections = a.connections;
    req.start_paused = a.start_paused;
    m.add(req).await.map_err(msg)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParsedUrls {
    pub valid: Vec<String>,
    pub rejected: Vec<String>,
}

fn is_http_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    ["http://", "https://"]
        .iter()
        .any(|p| lower.strip_prefix(p).is_some_and(|rest| !rest.is_empty() && !rest.starts_with('/')))
}

/// One URL per line. Blank lines are skipped; duplicates are kept once.
pub fn parse_urls(text: &str) -> ParsedUrls {
    let mut out = ParsedUrls { valid: Vec::new(), rejected: Vec::new() };
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let bucket = if is_http_url(line) { &mut out.valid } else { &mut out.rejected };
        if !bucket.iter().any(|x| x == line) {
            bucket.push(line.to_string());
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatchResult {
    pub added: Vec<String>,
    pub rejected: Vec<String>,
}

pub async fn add_batch(m: &ManagerHandle, text: &str, save_dir: Option<String>) -> Result<BatchResult, String> {
    let parsed = parse_urls(text);
    let save_dir = given(save_dir);
    let mut added = Vec::new();
    let mut rejected = parsed.rejected;
    for url in parsed.valid {
        let args = AddArgs { url: url.clone(), save_dir: save_dir.clone(), ..Default::default() };
        match add(m, args).await {
            Ok(id) => added.push(id),
            Err(_) => rejected.push(url),
        }
    }
    Ok(BatchResult { added, rejected })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Suggestion {
    pub filename: String,
    pub size: Option<u64>,
    pub category: String,
    pub resumable: bool,
}

/// Ask the server what this URL is, to pre-fill the Add dialog.
pub async fn suggest(url: &str, cfg: &EngineConfig) -> Result<Suggestion, String> {
    let url = url.trim();
    let client = build_client(cfg);
    let p = odm_engine::probe(&client, url, &Default::default()).await.map_err(msg)?;
    let name = p
        .filename
        .clone()
        .or_else(|| filename::from_url(&p.final_url))
        .or_else(|| filename::from_url(url))
        .unwrap_or_else(|| "download".into());
    Ok(Suggestion {
        category: category::detect(&name, p.mime.as_deref()).as_str().to_string(),
        filename: name,
        size: p.size,
        resumable: p.supports_range,
    })
}

pub async fn apply_settings(m: &ManagerHandle, s: &Settings) -> Result<(), String> {
    m.set_config(s.to_engine_config()).await.map_err(msg)
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p odm-appcore --test api`
Expected: 8 passed.

Note on `parse_urls_filters_and_dedupes`: `https://` (nothing after the scheme) is rejected, and `HTTPS://D.com/4` is accepted as written. The engine's own check is case-sensitive (`http://`/`https://`), so if `add_batch` on an upper-case scheme is rejected by the engine it lands in `rejected`; that is acceptable and not asserted here.

- [ ] **Step 6: Lint and commit**

Run: `cargo fmt --all && cargo clippy -p odm-appcore --all-targets -- -D warnings`
```bash
git add crates/odm-appcore
git commit -m "feat(appcore): UI DTOs and command functions"
```

---

### Task 6: `odm-appcore`: the event bridge

**Files:**
- Create: `crates/odm-appcore/src/bridge.rs`; modify `src/lib.rs` (add `pub mod bridge;`)
- Test: `crates/odm-appcore/tests/bridge.rs`

**Interfaces:**
- Consumes: `EngineEvent`, `ProgressSnapshot`, `ManagerHandle::{subscribe, get}`, `dto::{to_dto, DownloadDto, UiEvent}`.
- Produces:
  - `bridge::pump(rx: broadcast::Receiver<EngineEvent>, lookup: L, interval: Duration, emit: F)` (async) where `L: Fn(String) -> Fut`, `Fut: Future<Output = Option<DownloadDto>>`, `F: Fn(UiEvent)`. It returns when the channel closes.
    - `Added` and `State` events → `lookup(id)`; if found, `emit(Upsert)`. Any pending progress for that id is dropped first (the DTO is newer).
    - `Progress` events → kept as the latest snapshot per id; every `interval` the pending set is emitted as one `ProgressBatch` (sorted by id), and nothing is emitted when it is empty.
    - `Removed` → pending progress for that id is dropped; `emit(Removed)`.
    - `Lagged` → pending progress is cleared; `emit(Resync)`.
  - `bridge::run(manager: ManagerHandle, interval: Duration, emit: F)` (async): `pump` with `lookup` backed by `manager.get`.

- [ ] **Step 1: Write the failing tests**

`crates/odm-appcore/tests/bridge.rs`:
```rust
use odm_appcore::bridge;
use odm_appcore::dto::{to_dto, DownloadDto, UiEvent};
use odm_core::DownloadState;
use odm_engine::{AddRequest, EngineConfig, EngineEvent, ManagerHandle, ProgressSnapshot};
use odm_store::{DownloadRecord, Store};
use odm_testserver::{Faults, TestServer};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

type Log = Arc<Mutex<Vec<UiEvent>>>;

fn sink() -> (Log, impl Fn(UiEvent) + Send + 'static) {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let l2 = log.clone();
    (log, move |e| l2.lock().unwrap().push(e))
}

fn dto(id: &str, state: DownloadState) -> DownloadDto {
    let mut r = DownloadRecord::new_file(id, "https://e.com/f", "f", "C:/dl");
    r.state = state;
    to_dto(&r, 0)
}

fn progress(id: &str, bytes: u64) -> EngineEvent {
    EngineEvent::Progress(ProgressSnapshot { id: id.into(), bytes, total: Some(100), speed_bps: 1, eta_s: None, chunks: vec![] })
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(150)).await;
}

#[tokio::test]
async fn progress_is_coalesced_into_batches() {
    let (tx, rx) = broadcast::channel(64);
    let (log, emit) = sink();
    let task = tokio::spawn(bridge::pump(rx, |_id| async { None }, Duration::from_millis(50), emit));
    for b in 1..=10 {
        tx.send(progress("a", b)).unwrap();
        tx.send(progress("b", b * 2)).unwrap();
    }
    settle().await;
    drop(tx);
    task.await.unwrap();

    let events = log.lock().unwrap().clone();
    assert!(!events.is_empty());
    // Far fewer batches than the 20 progress events, and only the latest snapshot per id survives.
    assert!(events.len() <= 4, "{}", events.len());
    let UiEvent::ProgressBatch { items } = events.last().unwrap() else { panic!("{events:?}") };
    let ids: Vec<&str> = items.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "b"]);
    assert_eq!((items[0].bytes, items[1].bytes), (10, 20));
}

#[tokio::test]
async fn state_events_become_upserts_and_drop_stale_progress() {
    let (tx, rx) = broadcast::channel(64);
    let (log, emit) = sink();
    let lookup = |id: String| async move { Some(dto(&id, DownloadState::Completed)) };
    let task = tokio::spawn(bridge::pump(rx, lookup, Duration::from_secs(60), emit));
    tx.send(progress("a", 5)).unwrap();
    tx.send(EngineEvent::State { id: "a".into(), state: DownloadState::Completed, error_code: None, error_message: None }).unwrap();
    tx.send(EngineEvent::Removed { id: "b".into() }).unwrap();
    settle().await;
    drop(tx);
    task.await.unwrap();

    let events = log.lock().unwrap().clone();
    assert_eq!(
        events,
        vec![
            UiEvent::Upsert { download: dto("a", DownloadState::Completed) },
            UiEvent::Removed { id: "b".into() },
        ]
    );
}

#[tokio::test]
async fn unknown_ids_emit_nothing() {
    let (tx, rx) = broadcast::channel(8);
    let (log, emit) = sink();
    let task = tokio::spawn(bridge::pump(rx, |_id| async { None }, Duration::from_secs(60), emit));
    tx.send(EngineEvent::Added { id: "ghost".into() }).unwrap();
    settle().await;
    drop(tx);
    task.await.unwrap();
    assert!(log.lock().unwrap().is_empty());
}

#[tokio::test]
async fn lagged_emits_resync() {
    let (tx, rx) = broadcast::channel(4);
    // Overflow the channel before the bridge starts reading.
    for b in 0..50 {
        tx.send(progress("a", b)).unwrap();
    }
    let (log, emit) = sink();
    let task = tokio::spawn(bridge::pump(rx, |_id| async { None }, Duration::from_secs(60), emit));
    settle().await;
    drop(tx);
    task.await.unwrap();
    let events = log.lock().unwrap().clone();
    assert_eq!(events.first(), Some(&UiEvent::Resync), "{events:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_real_download_produces_upserts_and_batches() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(4 << 20, Faults { bytes_per_sec: Some(2 << 20), ..Default::default() }).await;
    let cfg = EngineConfig {
        download_dir: d.path().to_path_buf(),
        progress_interval: Duration::from_millis(20),
        ..Default::default()
    };
    let m = ManagerHandle::start(cfg, Store::open_in_memory().unwrap()).await.unwrap();
    let (log, emit) = sink();
    let task = tokio::spawn(bridge::run(m.clone(), Duration::from_millis(100), emit));

    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let done = log.lock().unwrap().iter().any(|e| {
            matches!(e, UiEvent::Upsert { download } if download.id == id && download.state == DownloadState::Completed)
        });
        if done {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "no completed upsert");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    task.abort();

    let events = log.lock().unwrap().clone();
    // The first thing the UI hears about a new download is its row. (Its state may
    // already be past `queued`: the engine starts it right after announcing it.)
    assert!(matches!(&events[0], UiEvent::Upsert { download } if download.id == id), "{:?}", events[0]);
    let batches = events.iter().filter(|e| matches!(e, UiEvent::ProgressBatch { .. })).count();
    assert!(batches >= 1, "no progress batches");
    // ~1 s of download at 20 ms progress ticks is ~50 engine events; batching at 100 ms keeps it near 10.
    assert!(batches <= 30, "{batches} batches: progress is not being coalesced");
    let last = events.iter().rev().find_map(|e| match e {
        UiEvent::Upsert { download } => Some(download.clone()),
        _ => None,
    });
    let last = last.unwrap();
    assert_eq!((last.state, last.bytes), (DownloadState::Completed, 4 << 20));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-appcore --test bridge`
Expected: compile error (`bridge` doesn't exist).

- [ ] **Step 3: Implement `src/bridge.rs`**

```rust
//! Turns the engine's event stream into what the UI consumes: list rows
//! on change, progress coalesced to a fixed rate, and a resync signal
//! when events were missed.
use odm_engine::{EngineEvent, ManagerHandle, ProgressSnapshot};
use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::time::MissedTickBehavior;

use crate::dto::{to_dto, DownloadDto, UiEvent};

pub async fn pump<L, Fut, F>(mut rx: broadcast::Receiver<EngineEvent>, lookup: L, interval: Duration, emit: F)
where
    L: Fn(String) -> Fut,
    Fut: Future<Output = Option<DownloadDto>>,
    F: Fn(UiEvent),
{
    let mut pending: HashMap<String, ProgressSnapshot> = HashMap::new();
    let mut tick = tokio::time::interval(interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if !pending.is_empty() {
                    let mut items: Vec<ProgressSnapshot> = pending.drain().map(|(_, v)| v).collect();
                    items.sort_by(|a, b| a.id.cmp(&b.id));
                    emit(UiEvent::ProgressBatch { items });
                }
            }
            event = rx.recv() => match event {
                Ok(EngineEvent::Progress(p)) => {
                    pending.insert(p.id.clone(), p);
                }
                Ok(EngineEvent::Added { id }) | Ok(EngineEvent::State { id, .. }) => {
                    pending.remove(&id);
                    if let Some(download) = lookup(id).await {
                        emit(UiEvent::Upsert { download });
                    }
                }
                Ok(EngineEvent::Removed { id }) => {
                    pending.remove(&id);
                    emit(UiEvent::Removed { id });
                }
                Err(RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "UI bridge lagged behind engine events; asking the UI to reload");
                    pending.clear();
                    emit(UiEvent::Resync);
                }
                Err(RecvError::Closed) => break,
            }
        }
    }
}

/// Bridge a running manager to `emit` until the manager goes away.
pub async fn run<F>(manager: ManagerHandle, interval: Duration, emit: F)
where
    F: Fn(UiEvent),
{
    let rx = manager.subscribe();
    let lookup = |id: String| {
        let m = manager.clone();
        async move { m.get(&id).await.ok().flatten().map(|(r, b)| to_dto(&r, b)) }
    };
    pump(rx, lookup, interval, emit).await;
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p odm-appcore --test bridge` (run it three times; it is timing-based)
Expected: 5 passed every time.

If `progress_is_coalesced_into_batches` sees only one id in the last batch: the last batch holds whatever arrived since the previous tick. Both ids are sent in the same burst before the first 50 ms tick after it, so both appear. If the test machine is slow, raise `settle()` rather than changing the assertion.

- [ ] **Step 5: Lint, full Rust suite, commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace -- --test-threads=4`
```bash
git add crates/odm-appcore
git commit -m "feat(appcore): event bridge with batched progress and resync"
```

---

### Task 7: App scaffold (Tauri shell + Svelte front end that opens a window)

**Files:**
- Modify: `Cargo.toml` (workspace `members = ["crates/*", "app/src-tauri"]`), `.gitignore` (add `app/dist/`, `app/node_modules/`, `app/src-tauri/gen/`)
- Create: `app/package.json`, `app/vite.config.ts`, `app/svelte.config.js`, `app/tsconfig.json`, `app/index.html`, `app/app-icon.svg`, `app/src/main.ts`, `app/src/App.svelte`, `app/src/app.css`, `app/src/vite-env.d.ts`
- Create: `app/src-tauri/Cargo.toml`, `build.rs`, `tauri.conf.json`, `capabilities/default.json`, `src/main.rs`, `src/lib.rs`, and generated `icons/`

**Interfaces:**
- Produces: a Tauri app crate `odm-app` (library `odm_app_lib` with `pub fn run()`), window label `main`, dev server on port 1420, and the theme CSS variables every later component uses: `--bg`, `--surface`, `--surface-2`, `--border`, `--text`, `--muted`, `--accent`, `--accent-light`, `--error`, `--success`, `--radius`.

- [ ] **Step 1: Front-end project files**

`app/package.json`:
```json
{
  "name": "odm-app",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "check": "svelte-check --tsconfig ./tsconfig.json",
    "test": "vitest run",
    "tauri": "tauri"
  }
}
```
Then install the dependencies (this resolves current versions and writes `pnpm-lock.yaml`):
```bash
cd app
pnpm add @tauri-apps/api@^2
pnpm add -D @tauri-apps/cli@^2 svelte@^5 @sveltejs/vite-plugin-svelte vite typescript svelte-check @tsconfig/svelte vitest
```

`app/vite.config.ts`:
```ts
import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022", outDir: "dist", emptyOutDir: true },
  test: { environment: "node", include: ["src/**/*.test.ts"] },
});
```

`app/svelte.config.js`:
```js
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

export default { preprocess: vitePreprocess() };
```

`app/tsconfig.json`:
```json
{
  "extends": "@tsconfig/svelte/tsconfig.json",
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "noUnusedLocals": true,
    "skipLibCheck": true,
    "types": ["svelte", "vite/client"]
  },
  "include": ["src/**/*.ts", "src/**/*.svelte"]
}
```

`app/src/vite-env.d.ts`:
```ts
/// <reference types="svelte" />
/// <reference types="vite/client" />
```

`app/index.html`:
```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>ODM</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

`app/src/main.ts`:
```ts
import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

const app = mount(App, { target: document.getElementById("app")! });

export default app;
```

`app/src/App.svelte`:
```svelte
<main class="empty">
  <h1>ODM</h1>
  <p>No downloads yet.</p>
</main>

<style>
  .empty {
    height: 100vh;
    display: grid;
    place-content: center;
    text-align: center;
    color: var(--muted);
  }
  h1 {
    color: var(--text);
    margin: 0 0 4px;
    font-size: 20px;
  }
</style>
```

`app/src/app.css`:
```css
:root {
  --bg: #0f0f14;
  --surface: #14141c;
  --surface-2: #1a1a24;
  --border: #1e1e28;
  --text: #e0e0e6;
  --muted: #6b6b80;
  --accent: #6366f1;
  --accent-light: #818cf8;
  --error: #ef4444;
  --success: #22c55e;
  --radius: 6px;
  color-scheme: dark;
}

* {
  box-sizing: border-box;
}

html,
body,
#app {
  height: 100%;
  margin: 0;
}

body {
  background: var(--bg);
  color: var(--text);
  font: 13px/1.4 "Segoe UI", system-ui, sans-serif;
  overflow: hidden;
  user-select: none;
}

button,
input,
select,
textarea {
  font: inherit;
  color: inherit;
}

button {
  background: var(--surface-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 6px 12px;
  cursor: pointer;
}

button:hover:not(:disabled) {
  border-color: var(--accent);
}

button:disabled {
  opacity: 0.45;
  cursor: default;
}

button.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}

input,
select,
textarea {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 6px 8px;
  user-select: text;
}

input:focus,
select:focus,
textarea:focus,
button:focus-visible {
  outline: 2px solid var(--accent-light);
  outline-offset: 1px;
}

::-webkit-scrollbar {
  width: 10px;
  height: 10px;
}

::-webkit-scrollbar-thumb {
  background: var(--border);
  border-radius: 5px;
}
```

`app/app-icon.svg` (source for the app and tray icons):
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <rect width="1024" height="1024" rx="220" fill="#6366f1"/>
  <path d="M512 232v388M344 468l168 168 168-168" fill="none" stroke="#fff" stroke-width="84" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M288 772h448" fill="none" stroke="#fff" stroke-width="84" stroke-linecap="round"/>
</svg>
```

- [ ] **Step 2: Tauri shell files**

`app/src-tauri/Cargo.toml`:
```toml
[package]
name = "odm-app"
edition.workspace = true
version.workspace = true
license.workspace = true

[lib]
name = "odm_app_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-single-instance = "2"
tauri-plugin-dialog = "2"
tauri-plugin-notification = "2"
tauri-plugin-opener = "2"
odm-core.workspace = true
odm-store.workspace = true
odm-engine.workspace = true
odm-appcore.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
tracing.workspace = true
```

`app/src-tauri/build.rs`:
```rust
fn main() {
    tauri_build::build()
}
```

`app/src-tauri/tauri.conf.json`:
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "ODM",
  "version": "0.1.0",
  "identifier": "com.odm.app",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "ODM",
        "width": 1180,
        "height": 720,
        "minWidth": 900,
        "minHeight": 560,
        "visible": false
      }
    ],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"]
  }
}
```
The window starts hidden; Rust shows it during setup (Task 9 adds the "start minimized" rule).

`app/src-tauri/capabilities/default.json`:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Permissions for the main window",
  "windows": ["main"],
  "permissions": ["core:default"]
}
```
The UI only calls this app's own commands and listens to events, both covered by `core:default`. Dialogs, notifications and opening files are done from Rust, so the web side needs no plugin permissions.

`app/src-tauri/src/main.rs`:
```rust
// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    odm_app_lib::run()
}
```

`app/src-tauri/src/lib.rs` (this task: just open the window):
```rust
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                w.show()?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ODM");
}
```

- [ ] **Step 3: Generate the icons**

```bash
cd app && pnpm tauri icon app-icon.svg
```
Expected: `app/src-tauri/icons/` now contains `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico` and others.

If this CLI version rejects SVG input, make a PNG first with PowerShell and pass that:
```bash
powershell -NoProfile -Command "Add-Type -AssemblyName System.Drawing; \$b = New-Object System.Drawing.Bitmap 1024,1024; \$g = [System.Drawing.Graphics]::FromImage(\$b); \$g.SmoothingMode = 'AntiAlias'; \$g.Clear([System.Drawing.Color]::FromArgb(99,102,241)); \$p = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), 84; \$p.StartCap = 'Round'; \$p.EndCap = 'Round'; \$p.LineJoin = 'Round'; \$g.DrawLine(\$p,512,232,512,620); \$g.DrawLines(\$p, @((New-Object System.Drawing.Point 344,468),(New-Object System.Drawing.Point 512,636),(New-Object System.Drawing.Point 680,468))); \$g.DrawLine(\$p,288,772,736,772); \$b.Save('app-icon.png')"
pnpm tauri icon app-icon.png
```

- [ ] **Step 4: Build both halves**

```bash
cd app && pnpm build && pnpm check
cd .. && cargo build -p odm-app
```
Expected: `app/dist/index.html` exists; `svelte-check` reports 0 errors; the Rust build finishes (the first Tauri build downloads and compiles many crates; allow several minutes).

- [ ] **Step 5: Launch smoke test**

```bash
cd app && pnpm tauri build --debug --no-bundle
cd .. && ( ./target/debug/odm-app.exe & ) ; sleep 6 ; tasklist | grep -i odm-app && echo "RUNNING" ; taskkill //IM odm-app.exe //F
```
Expected: `RUNNING` is printed (the process stayed alive for 6 s), then the process is killed.

- [ ] **Step 6: Workspace still healthy, commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings`
```bash
git add Cargo.toml Cargo.lock .gitignore app
git commit -m "feat(app): Tauri 2 shell and Svelte front-end scaffold"
```
Commit the generated `app/src-tauri/icons/` and `app/pnpm-lock.yaml`. Do not commit `app/node_modules`, `app/dist` or `app/src-tauri/gen`.

---

### Task 8: App state, commands and the event stream

**Files:**
- Modify: `crates/odm-appcore/src/paths.rs` (add `from_env`), `crates/odm-appcore/src/api.rs` (add `save_settings`), `crates/odm-appcore/src/dto.rs` (add `completed_download`)
- Test: `crates/odm-appcore/tests/api.rs`, `tests/settings.rs`
- Modify: `app/src-tauri/src/lib.rs`; create `app/src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: everything from Tasks 3–6.
- Produces:
  - `AppPaths::from_env() -> AppPaths`: uses the `ODM_DATA_DIR` environment variable when it is set and not empty, otherwise `default_location()`. (Lets a test or a second copy run without touching the real `%APPDATA%\ODM`.)
  - `api::save_settings(&ManagerHandle, path: &Path, Settings) -> Result<(Settings, Vec<String>), String>`: clamps, writes the file atomically, applies it to the manager, and returns the clamped settings plus any clamp warnings.
  - `dto::completed_download(&UiEvent) -> Option<&DownloadDto>`: `Some` only for an `Upsert` whose state is `completed`.
  - `pub struct AppState { pub manager: ManagerHandle, pub settings: std::sync::Mutex<Settings>, pub paths: AppPaths }`, managed by Tauri.
  - The event `odm://event`, whose payload is a `UiEvent`.
  - Tauri commands. Top-level argument names are camelCase in JS (`deleteFile`, `saveDir`); fields inside objects keep their Rust names (`save_dir`, `start_paused`):

    | Command | Arguments | Returns |
    |---|---|---|
    | `list_downloads` | – | `DownloadDto[]` |
    | `get_detail` | `id` | `DetailDto \| null` |
    | `add_download` | `args: AddArgs` | `string` (id) |
    | `add_batch` | `text`, `saveDir?` | `{ added: string[], rejected: string[] }` |
    | `suggest` | `url` | `Suggestion` |
    | `pause` / `resume` | `ids: string[]` | – |
    | `cancel` / `remove` | `ids: string[]`, `deleteFile: boolean` | – |
    | `pause_all` / `resume_all` | – | – |
    | `get_settings` | – | `Settings` |
    | `save_settings` | `settings: Settings` | `Settings` (clamped) |
    | `open_file` / `open_folder` | `id` | – |
    | `pick_folder` | `start?` | `string \| null` |

    Every command returns `Result<_, String>`; the string is a message fit to show the user.

- [ ] **Step 1: Write the failing tests** (appcore)

Append to `crates/odm-appcore/tests/api.rs`:
```rust
#[tokio::test(flavor = "multi_thread")]
async fn save_settings_clamps_persists_and_applies() {
    let data = tempfile::tempdir().unwrap();
    let dl = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = manager(data.path()).await;
    let path = data.path().join("settings.json");

    let wanted = Settings {
        download_dir: dl.path().to_string_lossy().into_owned(),
        max_connections: 500,
        ..Default::default()
    };
    let (saved, warnings) = api::save_settings(&m, &path, wanted).await.unwrap();
    assert_eq!(saved.max_connections, 32);
    assert_eq!(warnings.len(), 1);
    assert_eq!(Settings::load(&path).0, saved);

    let id = api::add(&m, AddArgs { url: s.url("/file"), ..Default::default() }).await.unwrap();
    wait_completed(&m, &id).await;
    assert!(dl.path().join("file").exists());
}

#[test]
fn completed_download_only_matches_completed_upserts() {
    use odm_appcore::dto::{completed_download, to_dto};
    let mut r = odm_store::DownloadRecord::new_file("a", "https://e.com/f", "f", "C:/dl");
    let queued = UiEvent::Upsert { download: to_dto(&r, 0) };
    r.state = DownloadState::Completed;
    let done = UiEvent::Upsert { download: to_dto(&r, 0) };
    assert!(completed_download(&queued).is_none());
    assert_eq!(completed_download(&done).unwrap().id, "a");
    assert!(completed_download(&UiEvent::Resync).is_none());
}
```

Append to `crates/odm-appcore/tests/settings.rs`:
```rust
#[test]
fn from_env_prefers_odm_data_dir() {
    // Env vars are process-wide: this is the only test that touches ODM_DATA_DIR.
    std::env::set_var("ODM_DATA_DIR", "X:/elsewhere");
    assert_eq!(AppPaths::from_env(), AppPaths::at("X:/elsewhere"));
    std::env::set_var("ODM_DATA_DIR", "");
    assert_eq!(AppPaths::from_env(), AppPaths::default_location());
    std::env::remove_var("ODM_DATA_DIR");
    assert_eq!(AppPaths::from_env(), AppPaths::default_location());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p odm-appcore`
Expected: compile errors (`save_settings`, `completed_download`, `from_env`).

- [ ] **Step 3: Implement the appcore additions**

`paths.rs`, inside `impl AppPaths`:
```rust
    /// `ODM_DATA_DIR` if set and not empty, otherwise the default location.
    pub fn from_env() -> Self {
        match std::env::var_os("ODM_DATA_DIR") {
            Some(v) if !v.is_empty() => Self::at(PathBuf::from(v)),
            _ => Self::default_location(),
        }
    }
```

`api.rs`:
```rust
/// Clamp, write the file, then apply to the running manager.
pub async fn save_settings(
    m: &ManagerHandle,
    path: &std::path::Path,
    mut s: Settings,
) -> Result<(Settings, Vec<String>), String> {
    let warnings = s.clamp();
    s.save(path).map_err(|e| format!("could not save settings: {e}"))?;
    apply_settings(m, &s).await?;
    Ok((s, warnings))
}
```

`dto.rs`:
```rust
/// The download in `ev`, if this event says it just completed.
pub fn completed_download(ev: &UiEvent) -> Option<&DownloadDto> {
    match ev {
        UiEvent::Upsert { download } if download.state == DownloadState::Completed => Some(download),
        _ => None,
    }
}
```

Run: `cargo test -p odm-appcore` → all pass.

- [ ] **Step 4: Implement `app/src-tauri/src/commands.rs`**

```rust
use odm_appcore::api::{self, AddArgs, BatchResult, Suggestion};
use odm_appcore::dto::{DetailDto, DownloadDto};
use odm_appcore::Settings;
use std::path::PathBuf;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::AppState;

type Res<T> = Result<T, String>;

fn msg(e: odm_core::OdmError) -> String {
    e.to_string()
}

#[tauri::command]
pub async fn list_downloads(state: State<'_, AppState>) -> Res<Vec<DownloadDto>> {
    api::list(&state.manager).await
}

#[tauri::command]
pub async fn get_detail(state: State<'_, AppState>, id: String) -> Res<Option<DetailDto>> {
    api::detail(&state.manager, &id).await
}

#[tauri::command]
pub async fn add_download(state: State<'_, AppState>, args: AddArgs) -> Res<String> {
    api::add(&state.manager, args).await
}

#[tauri::command]
pub async fn add_batch(state: State<'_, AppState>, text: String, save_dir: Option<String>) -> Res<BatchResult> {
    api::add_batch(&state.manager, &text, save_dir).await
}

#[tauri::command]
pub async fn suggest(state: State<'_, AppState>, url: String) -> Res<Suggestion> {
    let cfg = state.settings.lock().unwrap().to_engine_config();
    api::suggest(&url, &cfg).await
}

#[tauri::command]
pub async fn pause(state: State<'_, AppState>, ids: Vec<String>) -> Res<()> {
    for id in ids {
        state.manager.pause(&id).await.map_err(msg)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn resume(state: State<'_, AppState>, ids: Vec<String>) -> Res<()> {
    for id in ids {
        state.manager.resume(&id).await.map_err(msg)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel(state: State<'_, AppState>, ids: Vec<String>, delete_file: bool) -> Res<()> {
    for id in ids {
        state.manager.cancel(&id, delete_file).await.map_err(msg)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn remove(state: State<'_, AppState>, ids: Vec<String>, delete_file: bool) -> Res<()> {
    for id in ids {
        state.manager.remove(&id, delete_file).await.map_err(msg)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn pause_all(state: State<'_, AppState>) -> Res<()> {
    state.manager.pause_all().await.map_err(msg)
}

#[tauri::command]
pub async fn resume_all(state: State<'_, AppState>) -> Res<()> {
    state.manager.resume_all().await.map_err(msg)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: Settings) -> Res<Settings> {
    let (saved, warnings) = api::save_settings(&state.manager, &state.paths.settings, settings).await?;
    for w in warnings {
        tracing::warn!("settings: {w}");
    }
    *state.settings.lock().unwrap() = saved.clone();
    Ok(saved)
}

async fn file_path(state: &State<'_, AppState>, id: &str) -> Res<(PathBuf, PathBuf)> {
    let (rec, _) = state.manager.get(id).await.map_err(msg)?.ok_or("That download no longer exists.")?;
    let dir = PathBuf::from(&rec.save_dir);
    Ok((dir.join(&rec.filename), dir))
}

#[tauri::command]
pub async fn open_file(app: AppHandle, state: State<'_, AppState>, id: String) -> Res<()> {
    let (file, _) = file_path(&state, &id).await?;
    if !file.exists() {
        return Err("The file is not there. It may have been moved or deleted.".into());
    }
    app.opener().open_path(file.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_folder(app: AppHandle, state: State<'_, AppState>, id: String) -> Res<()> {
    let (file, dir) = file_path(&state, &id).await?;
    if file.exists() {
        app.opener().reveal_item_in_dir(&file).map_err(|e| e.to_string())
    } else if dir.exists() {
        app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
    } else {
        Err("The folder is not there yet.".into())
    }
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle, start: Option<String>) -> Res<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app.dialog().file();
        if let Some(s) = start.filter(|s| !s.trim().is_empty()) {
            dialog = dialog.set_directory(s);
        }
        dialog
            .blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())
}
```

- [ ] **Step 5: Implement `app/src-tauri/src/lib.rs`**

```rust
mod commands;

use odm_appcore::dto::UiEvent;
use odm_appcore::{bridge, logging, AppPaths, Settings};
use odm_engine::ManagerHandle;
use odm_store::Store;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager, RunEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

pub const EVENT: &str = "odm://event";
/// Progress reaches the UI at most 4 times per second.
const UI_PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

pub struct AppState {
    pub manager: ManagerHandle,
    pub settings: Mutex<Settings>,
    pub paths: AppPaths,
}

fn start_engine(paths: &AppPaths, settings: &Settings) -> Result<ManagerHandle, String> {
    let store = Store::open(&paths.db).map_err(|e| format!("ODM could not open its database.\n\n{e}"))?;
    tauri::async_runtime::block_on(ManagerHandle::start(settings.to_engine_config(), store))
        .map_err(|e| format!("ODM could not start its download engine.\n\n{e}"))
}

pub fn run() {
    let paths = AppPaths::from_env();
    let _ = paths.ensure();
    // Kept alive until the process exits, so buffered log lines are flushed.
    let _log_guard = logging::init(&paths.logs).ok();
    let (settings, warnings) = Settings::load(&paths.settings);
    for w in &warnings {
        tracing::warn!("settings: {w}");
    }
    tracing::info!(data_dir = %paths.data_dir.display(), "ODM starting");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let manager = match start_engine(&paths, &settings) {
                Ok(m) => m,
                Err(message) => {
                    tracing::error!("{message}");
                    app.dialog().message(&message).title("ODM").kind(MessageDialogKind::Error).blocking_show();
                    return Err(message.into());
                }
            };
            app.manage(AppState { manager: manager.clone(), settings: Mutex::new(settings), paths });

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(bridge::run(manager, UI_PROGRESS_INTERVAL, move |ev: UiEvent| {
                if let Err(e) = handle.emit(EVENT, &ev) {
                    tracing::warn!("could not send an event to the window: {e}");
                }
            }));

            if let Some(w) = app.get_webview_window("main") {
                w.show()?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_downloads,
            commands::get_detail,
            commands::add_download,
            commands::add_batch,
            commands::suggest,
            commands::pause,
            commands::resume,
            commands::cancel,
            commands::remove,
            commands::pause_all,
            commands::resume_all,
            commands::get_settings,
            commands::save_settings,
            commands::open_file,
            commands::open_folder,
            commands::pick_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ODM");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            if let Some(state) = app.try_state::<AppState>() {
                // Stops downloads cleanly; they are left queued and resume next start.
                tauri::async_runtime::block_on(state.manager.shutdown());
            }
        }
    });
}
```

- [ ] **Step 6: Build and smoke-test with a throwaway data directory**

```bash
cargo build -p odm-app
cd app && pnpm tauri build --debug --no-bundle && cd ..
export ODM_DATA_DIR="$TEMP/odm-smoke-task8"; rm -rf "$ODM_DATA_DIR"
( ./target/debug/odm-app.exe & ) ; sleep 8
ls "$ODM_DATA_DIR" "$ODM_DATA_DIR/logs"; grep -c "ODM starting" "$ODM_DATA_DIR"/logs/odm.*.log
taskkill //IM odm-app.exe //F
```
Expected: the listing shows `odm.db` and `logs`; the logs folder has one `odm.<date>.log`; the grep prints `1`.

- [ ] **Step 7: Lint and commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p odm-appcore`
```bash
git add crates/odm-appcore app/src-tauri
git commit -m "feat(app): engine wiring, commands and the UI event stream"
```

---

### Task 9: Tray, window behaviour, quit flow, notifications

**Files:**
- Create: `app/src-tauri/src/shell.rs`
- Modify: `app/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `AppState`, `dto::completed_download`, `ManagerHandle::{active_count, pause_all, resume_all, shutdown}`.
- Produces:
  - `shell::show_main(&AppHandle)`: show, un-minimize and focus the main window.
  - `shell::setup(&mut tauri::App) -> tauri::Result<()>`: builds the tray (menu: Show ODM, Pause all, Resume all, separator, Quit) and shows the window unless `start_minimized` is on.
  - `shell::on_window_event(&tauri::Window, &tauri::WindowEvent)`: on close, hide to tray when `close_to_tray` is on, otherwise start the quit flow.
  - `shell::request_quit(&AppHandle)`: if downloads are active, ask "Quit" / "Keep running"; on Quit, shut the manager down and exit.
  - `shell::on_ui_event(&AppHandle, &UiEvent)`: shows a "Download complete" notification when `notifications` is on.

**Deviation from the spec, recorded on purpose:** spec §9 says clicking the notification opens the file's folder. The Tauri notification plugin doesn't deliver click events on Windows desktop, so the notification is informational only. The folder is one click away in the app (Open folder).

- [ ] **Step 1: Implement `app/src-tauri/src/shell.rs`**

```rust
use odm_appcore::dto::{completed_download, UiEvent};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, Window, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_notification::NotificationExt;

use crate::AppState;

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn setup(app: &mut App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show ODM", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause_all", "Pause all", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume_all", "Resume all", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &pause, &resume, &PredefinedMenuItem::separator(app)?, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("ODM")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "pause_all" | "resume_all" => {
                let app = app.clone();
                let pause = event.id.as_ref() == "pause_all";
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    let r = if pause { state.manager.pause_all().await } else { state.manager.resume_all().await };
                    if let Err(e) = r {
                        tracing::warn!("tray action failed: {e}");
                    }
                });
            }
            "quit" => request_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;

    let start_minimized = app.state::<AppState>().settings.lock().unwrap().start_minimized;
    if !start_minimized {
        show_main(app.handle());
    }
    Ok(())
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        // Either way the window does not close by itself: hide it, or run the quit flow.
        api.prevent_close();
        let app = window.app_handle();
        let to_tray = app.state::<AppState>().settings.lock().unwrap().close_to_tray;
        if to_tray {
            let _ = window.hide();
        } else {
            request_quit(app);
        }
    }
}

pub fn request_quit(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let manager = app.state::<AppState>().manager.clone();
        let active = manager.active_count().await.unwrap_or(0);
        if active > 0 {
            let text = if active == 1 {
                "1 download is still running. It will continue the next time you open ODM.".to_string()
            } else {
                format!("{active} downloads are still running. They will continue the next time you open ODM.")
            };
            let dialog_app = app.clone();
            let confirmed = tauri::async_runtime::spawn_blocking(move || {
                dialog_app
                    .dialog()
                    .message(text)
                    .title("Quit ODM")
                    .kind(MessageDialogKind::Warning)
                    .buttons(MessageDialogButtons::OkCancelCustom("Quit".into(), "Keep running".into()))
                    .blocking_show()
            })
            .await
            .unwrap_or(false);
            if !confirmed {
                return;
            }
        }
        manager.shutdown().await;
        app.exit(0);
    });
}

pub fn on_ui_event(app: &AppHandle, ev: &UiEvent) {
    let Some(download) = completed_download(ev) else { return };
    if !app.state::<AppState>().settings.lock().unwrap().notifications {
        return;
    }
    if let Err(e) = app.notification().builder().title("Download complete").body(&download.filename).show() {
        tracing::debug!("notification failed: {e}");
    }
}
```

- [ ] **Step 2: Wire it into `lib.rs`**

- Add `mod shell;`.
- In the bridge closure, call the notifier before emitting:
```rust
            tauri::async_runtime::spawn(bridge::run(manager, UI_PROGRESS_INTERVAL, move |ev: UiEvent| {
                shell::on_ui_event(&handle, &ev);
                if let Err(e) = handle.emit(EVENT, &ev) {
                    tracing::warn!("could not send an event to the window: {e}");
                }
            }));
```
- Replace the `if let Some(w) = app.get_webview_window("main") { w.show()?; }` block at the end of `setup` with `shell::setup(app)?;`.
- Use `shell::show_main(app)` inside the single-instance callback.
- Add `.on_window_event(shell::on_window_event)` to the builder (before `.invoke_handler`).

- [ ] **Step 3: Build**

Run: `cargo build -p odm-app && cargo clippy --workspace --all-targets -- -D warnings`
Expected: builds with no warnings.

- [ ] **Step 4: Smoke-test single instance and clean exit**

```bash
cd app && pnpm tauri build --debug --no-bundle && cd ..
export ODM_DATA_DIR="$TEMP/odm-smoke-task9"; rm -rf "$ODM_DATA_DIR"
( ./target/debug/odm-app.exe & ) ; sleep 6
( ./target/debug/odm-app.exe & ) ; sleep 4
tasklist | grep -ci "odm-app.exe"
taskkill //IM odm-app.exe //F
```
Expected: the count printed is `1` (the second launch handed over to the first and exited).

- [ ] **Step 5: Manual checks (record the result of each in the report)**

Run `cd app && pnpm tauri dev`, then:
1. The window opens. A tray icon with the ODM arrow appears.
2. Click the window's close button: the window hides, the tray icon stays. Left-click the tray icon: the window comes back.
3. Tray → Quit with no downloads running: the app exits without asking.

If you cannot operate a GUI, say so in the report and leave these three for the controller.

- [ ] **Step 6: Commit**

```bash
git add app/src-tauri
git commit -m "feat(app): tray, close-to-tray, single instance, quit flow, notifications"
```

---

### Task 10: Front-end foundation (types, formatting, list model, API, store)

**Files:**
- Create: `app/src/lib/types.ts`, `format.ts`, `model.ts`, `api.ts`, `downloads.svelte.ts`
- Test: `app/src/lib/format.test.ts`, `app/src/lib/model.test.ts`

**Interfaces:**
- Consumes: the Tauri commands and the `odm://event` payload from Task 8. The TS types mirror `DownloadDto`, `DetailDto`, `UiEvent`, `Settings`, `AddArgs`, `Suggestion`, `BatchResult` field for field (snake_case).
- Produces:
  - `types.ts`: `Download`, `DownloadState`, `ChunkView`, `ChunkState`, `Progress`, `Detail`, `UiEvent`, `Settings`, `AddArgs`, `Suggestion`, `BatchResult`.
  - `format.ts`: `formatBytes(n)`, `formatSpeed(bps)`, `formatEta(seconds | null)`, `percent(bytes, total | null): number | null`, `formatDate(ms)`, `stateLabel(state)`.
  - `model.ts`: `Row` (a `Download` plus `speed_bps`, `eta_s`, `chunks`), `CATEGORIES`, `Filter`, `SortKey`, `isActive`, `toRow`, `replaceAll`, `applyEvent`, `matches`, `counts`, `visible`, `canPause`, `canResume`, `canCancel`, `segments`, `parseUrls`.
  - `api.ts`: one function per command, plus `onEvent(cb)`.
  - `downloads.svelte.ts`: the singleton `downloads` store (`rows`, `filter`, `sortKey`, `sortDir`, `selected`, `error`, derived `visible` / `counts` / `selectedRows`, methods `init`, `reload`, `handle`, `select`, `toggleSort`, `run`).

- [ ] **Step 1: Write the failing tests**

`app/src/lib/format.test.ts`:
```ts
import { describe, expect, it } from "vitest";
import { formatBytes, formatEta, formatSpeed, percent, stateLabel } from "./format";

describe("formatBytes", () => {
  it("picks a readable unit", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1536 * 1024)).toBe("1.5 MB");
    expect(formatBytes(3 * 1024 ** 3)).toBe("3.00 GB");
  });
});

describe("formatSpeed", () => {
  it("shows a dash when idle", () => {
    expect(formatSpeed(0)).toBe("—");
    expect(formatSpeed(2048)).toBe("2.0 KB/s");
  });
});

describe("formatEta", () => {
  it("uses the two largest units", () => {
    expect(formatEta(null)).toBe("—");
    expect(formatEta(0)).toBe("0 s");
    expect(formatEta(59)).toBe("59 s");
    expect(formatEta(61)).toBe("1 min 1 s");
    expect(formatEta(3700)).toBe("1 h 1 min");
    expect(formatEta(90000)).toBe("1 d 1 h");
  });
});

describe("percent", () => {
  it("is a clamped whole-number-friendly value", () => {
    expect(percent(50, 200)).toBe(25);
    expect(percent(300, 200)).toBe(100);
  });
  it("percent_is_null_without_total", () => {
    expect(percent(123, null)).toBeNull();
    expect(percent(0, 0)).toBeNull();
  });
});

describe("stateLabel", () => {
  it("is sentence case", () => {
    expect(stateLabel("downloading")).toBe("Downloading");
    expect(stateLabel("probing")).toBe("Starting");
  });
});
```

`app/src/lib/model.test.ts`:
```ts
import { describe, expect, it } from "vitest";
import {
  applyEvent,
  canCancel,
  canPause,
  canResume,
  counts,
  matches,
  parseUrls,
  replaceAll,
  segments,
  visible,
} from "./model";
import type { Download } from "./types";

function dl(id: string, patch: Partial<Download> = {}): Download {
  return {
    id,
    url: `https://e.com/${id}`,
    filename: `${id}.bin`,
    save_dir: "C:/dl",
    size: 1000,
    bytes: 0,
    mime: null,
    category: "general",
    state: "queued",
    error_code: null,
    error_message: null,
    max_connections: 8,
    kind: "file",
    page_url: null,
    page_title: null,
    created_at: 1,
    started_at: null,
    finished_at: null,
    ...patch,
  };
}

describe("applyEvent", () => {
  it("upsert adds and replaces rows", () => {
    let rows = replaceAll([]);
    rows = applyEvent(rows, { type: "upsert", download: dl("a") }).rows;
    rows = applyEvent(rows, { type: "upsert", download: dl("a", { state: "downloading" }) }).rows;
    expect(rows.size).toBe(1);
    expect(rows.get("a")!.state).toBe("downloading");
  });

  it("progress updates bytes, speed, eta and chunks of known rows only", () => {
    let rows = replaceAll([dl("a", { state: "downloading", size: null })]);
    const chunk = { idx: 0, start: 0, end: 999, written: 400, state: "active" as const };
    rows = applyEvent(rows, {
      type: "progress_batch",
      items: [
        { id: "a", bytes: 400, total: 1000, speed_bps: 50, eta_s: 12, chunks: [chunk] },
        { id: "ghost", bytes: 1, total: 1, speed_bps: 1, eta_s: 1, chunks: [] },
      ],
    }).rows;
    const a = rows.get("a")!;
    expect([a.bytes, a.size, a.speed_bps, a.eta_s]).toEqual([400, 1000, 50, 12]);
    expect(a.chunks).toEqual([chunk]);
    expect(rows.has("ghost")).toBe(false);
  });

  it("a row that stops being active loses its live speed and chunks", () => {
    let rows = replaceAll([dl("a", { state: "downloading" })]);
    rows = applyEvent(rows, {
      type: "progress_batch",
      items: [{ id: "a", bytes: 10, total: 1000, speed_bps: 99, eta_s: 5, chunks: [] }],
    }).rows;
    rows = applyEvent(rows, { type: "upsert", download: dl("a", { state: "paused", bytes: 10 }) }).rows;
    const a = rows.get("a")!;
    expect([a.speed_bps, a.eta_s, a.chunks]).toEqual([0, null, null]);
  });

  it("removed deletes the row", () => {
    const rows = applyEvent(replaceAll([dl("a"), dl("b")]), { type: "removed", id: "a" }).rows;
    expect([...rows.keys()]).toEqual(["b"]);
  });

  it("resync asks for a reload and leaves rows alone", () => {
    const before = replaceAll([dl("a")]);
    const r = applyEvent(before, { type: "resync" });
    expect(r.resync).toBe(true);
    expect(r.rows).toBe(before);
  });

  it("resync_replaces_the_list", () => {
    // What the store does after a resync: the fresh list wins completely.
    const stale = replaceAll([dl("a", { state: "downloading" }), dl("gone")]);
    const fresh = replaceAll([dl("a", { state: "completed", bytes: 1000 }), dl("new")]);
    expect(stale.has("gone")).toBe(true);
    expect([...fresh.keys()].sort()).toEqual(["a", "new"]);
    expect(fresh.get("a")!.state).toBe("completed");
  });
});

describe("filters and counts", () => {
  const rows = replaceAll([
    dl("a", { state: "downloading", category: "video" }),
    dl("b", { state: "paused" }),
    dl("c", { state: "completed", category: "video" }),
    dl("d", { state: "failed" }),
    dl("e", { state: "probing" }),
  ]);

  it("matches by status group and category", () => {
    const ids = (f: Parameters<typeof matches>[1]) => [...rows.values()].filter((r) => matches(r, f)).map((r) => r.id);
    expect(ids("all")).toHaveLength(5);
    expect(ids("active")).toEqual(["a", "e"]);
    expect(ids("queued")).toEqual(["b"]);
    expect(ids("completed")).toEqual(["c"]);
    expect(ids("failed")).toEqual(["d"]);
    expect(ids("cat:video")).toEqual(["a", "c"]);
  });

  it("counts every filter", () => {
    const c = counts(rows);
    expect([c.all, c.active, c.queued, c.completed, c.failed]).toEqual([5, 2, 1, 1, 1]);
    expect(c["cat:video"]).toBe(2);
    expect(c["cat:audio"]).toBe(0);
  });

  it("sorts newest first by default and by name on request", () => {
    const r = replaceAll([dl("b", { created_at: 1 }), dl("a", { created_at: 2 })]);
    expect(visible(r, "all", "created_at", "desc").map((x) => x.id)).toEqual(["a", "b"]);
    expect(visible(r, "all", "filename", "asc").map((x) => x.id)).toEqual(["a", "b"]);
    expect(visible(r, "all", "filename", "desc").map((x) => x.id)).toEqual(["b", "a"]);
  });
});

describe("actions allowed per state", () => {
  it("follows the state machine", () => {
    const row = (state: Download["state"]) => replaceAll([dl("x", { state })]).get("x")!;
    expect(canPause(row("downloading")) && canPause(row("queued"))).toBe(true);
    expect(canPause(row("paused")) || canPause(row("completed"))).toBe(false);
    expect(canResume(row("paused")) && canResume(row("failed"))).toBe(true);
    expect(canResume(row("downloading"))).toBe(false);
    expect(canCancel(row("paused")) && canCancel(row("downloading"))).toBe(true);
    expect(canCancel(row("completed")) || canCancel(row("cancelled"))).toBe(false);
  });
});

describe("segments", () => {
  it("sizes each chunk by its share of the file", () => {
    const s = segments(
      [
        { idx: 0, start: 0, end: 249, written: 250, state: "done" },
        { idx: 1, start: 250, end: 999, written: 375, state: "active" },
      ],
      1000,
    )!;
    expect(s.map((x) => x.widthPct)).toEqual([25, 75]);
    expect(s.map((x) => x.fillPct)).toEqual([100, 50]);
  });

  it("is null when the size is unknown", () => {
    expect(segments([{ idx: 0, start: 0, end: null, written: 5, state: "active" }], null)).toBeNull();
    expect(segments([], 1000)).toBeNull();
  });
});

describe("parseUrls", () => {
  it("keeps http(s) lines once and reports the rest", () => {
    const p = parseUrls("https://a.com/1\n\n  http://b.com/2 \nftp://c\nhttps://a.com/1\nnope\nhttps://\n");
    expect(p.valid).toEqual(["https://a.com/1", "http://b.com/2"]);
    expect(p.rejected).toEqual(["ftp://c", "nope", "https://"]);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd app && pnpm test`
Expected: FAIL, modules `./format` and `./model` not found.

- [ ] **Step 3: Implement `types.ts`**

```ts
export type DownloadState =
  | "queued"
  | "probing"
  | "downloading"
  | "paused"
  | "merging"
  | "completed"
  | "failed"
  | "cancelled";

export type ChunkState = "pending" | "active" | "retrying" | "done" | "failed";

export interface Download {
  id: string;
  url: string;
  filename: string;
  save_dir: string;
  size: number | null;
  bytes: number;
  mime: string | null;
  category: string;
  state: DownloadState;
  error_code: string | null;
  error_message: string | null;
  max_connections: number;
  kind: string;
  page_url: string | null;
  page_title: string | null;
  created_at: number;
  started_at: number | null;
  finished_at: number | null;
}

export interface ChunkView {
  idx: number;
  start: number;
  end: number | null;
  written: number;
  state: ChunkState;
}

export interface Progress {
  id: string;
  bytes: number;
  total: number | null;
  speed_bps: number;
  eta_s: number | null;
  chunks: ChunkView[];
}

export interface Detail {
  download: Download;
  final_url: string | null;
  chunks: ChunkView[];
  header_names: string[];
}

export type UiEvent =
  | { type: "upsert"; download: Download }
  | { type: "removed"; id: string }
  | { type: "progress_batch"; items: Progress[] }
  | { type: "resync" };

export interface Settings {
  download_dir: string;
  category_folders: Record<string, string>;
  max_concurrent: number;
  max_connections: number;
  max_attempts: number;
  speed_limit_kbps: number;
  connect_timeout_s: number;
  read_timeout_s: number;
  ffmpeg_path: string;
  capture_enabled: boolean;
  min_capture_bytes: number;
  skip_hosts: string[];
  close_to_tray: boolean;
  start_minimized: boolean;
  notifications: boolean;
}

export interface AddArgs {
  url: string;
  filename?: string | null;
  save_dir?: string | null;
  connections?: number | null;
  category?: string | null;
  checksum?: string | null;
  start_paused?: boolean;
}

export interface Suggestion {
  filename: string;
  size: number | null;
  category: string;
  resumable: boolean;
}

export interface BatchResult {
  added: string[];
  rejected: string[];
}
```

- [ ] **Step 4: Implement `format.ts`**

```ts
import type { DownloadState } from "./types";

const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(n: number): string {
  let v = n;
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i += 1;
  }
  if (i === 0) return `${Math.round(v)} B`;
  return `${v.toFixed(i >= 3 ? 2 : 1)} ${UNITS[i]}`;
}

export function formatSpeed(bps: number): string {
  return bps > 0 ? `${formatBytes(bps)}/s` : "—";
}

export function formatEta(seconds: number | null): string {
  if (seconds === null) return "—";
  const s = Math.max(0, Math.round(seconds));
  const parts: [number, string][] = [
    [Math.floor(s / 86400), "d"],
    [Math.floor((s % 86400) / 3600), "h"],
    [Math.floor((s % 3600) / 60), "min"],
    [s % 60, "s"],
  ];
  const first = parts.findIndex(([v]) => v > 0);
  if (first === -1) return "0 s";
  return parts
    .slice(first, first + 2)
    .filter(([v]) => v > 0)
    .map(([v, u]) => `${v} ${u}`)
    .join(" ");
}

/** 0–100, or null when the total size is unknown. */
export function percent(bytes: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.min(100, (bytes / total) * 100);
}

export function formatDate(ms: number): string {
  const d = new Date(ms);
  const sameDay = d.toDateString() === new Date().toDateString();
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return sameDay ? time : `${d.toLocaleDateString()} ${time}`;
}

const LABELS: Record<DownloadState, string> = {
  queued: "Queued",
  probing: "Starting",
  downloading: "Downloading",
  paused: "Paused",
  merging: "Merging",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled",
};

export function stateLabel(state: DownloadState): string {
  return LABELS[state];
}
```

- [ ] **Step 5: Implement `model.ts`**

```ts
import type { ChunkState, ChunkView, Download, DownloadState, UiEvent } from "./types";

/** A list row: the stored download plus live values from progress events. */
export interface Row extends Download {
  speed_bps: number;
  eta_s: number | null;
  chunks: ChunkView[] | null;
}

export const CATEGORIES = ["video", "audio", "document", "compressed", "image", "program", "general"] as const;

export type StatusFilter = "all" | "active" | "queued" | "completed" | "failed";
export type Filter = StatusFilter | `cat:${string}`;
export type SortKey = "filename" | "size" | "progress" | "speed_bps" | "state" | "created_at";
export type SortDir = "asc" | "desc";

const ACTIVE: DownloadState[] = ["probing", "downloading", "merging"];
const DONE: DownloadState[] = ["completed", "failed", "cancelled"];

export function isActive(state: DownloadState): boolean {
  return ACTIVE.includes(state);
}

export function toRow(d: Download, prev?: Row): Row {
  // Live values only carry over while the download is still running.
  const live = isActive(d.state) ? prev : undefined;
  return {
    ...d,
    // While running, the last progress event is newer than the stored byte count.
    bytes: live ? Math.max(d.bytes, live.bytes) : d.bytes,
    speed_bps: live ? live.speed_bps : 0,
    eta_s: live ? live.eta_s : null,
    chunks: live ? live.chunks : null,
  };
}

export function replaceAll(list: Download[]): Map<string, Row> {
  return new Map(list.map((d) => [d.id, toRow(d)]));
}

export function applyEvent(rows: Map<string, Row>, ev: UiEvent): { rows: Map<string, Row>; resync: boolean } {
  switch (ev.type) {
    case "upsert": {
      const next = new Map(rows);
      next.set(ev.download.id, toRow(ev.download, rows.get(ev.download.id)));
      return { rows: next, resync: false };
    }
    case "removed": {
      if (!rows.has(ev.id)) return { rows, resync: false };
      const next = new Map(rows);
      next.delete(ev.id);
      return { rows: next, resync: false };
    }
    case "progress_batch": {
      let next: Map<string, Row> | null = null;
      for (const p of ev.items) {
        const row = rows.get(p.id);
        if (!row) continue;
        next ??= new Map(rows);
        next.set(p.id, {
          ...row,
          bytes: p.bytes,
          size: p.total ?? row.size,
          speed_bps: p.speed_bps,
          eta_s: p.eta_s,
          chunks: p.chunks,
        });
      }
      return { rows: next ?? rows, resync: false };
    }
    case "resync":
      return { rows, resync: true };
  }
}

export function matches(row: Row, filter: Filter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "active":
      return isActive(row.state);
    case "queued":
      return row.state === "queued" || row.state === "paused";
    case "completed":
      return row.state === "completed";
    case "failed":
      return row.state === "failed" || row.state === "cancelled";
    default:
      return row.category === filter.slice(4);
  }
}

export function counts(rows: Map<string, Row>): Record<string, number> {
  const out: Record<string, number> = { all: 0, active: 0, queued: 0, completed: 0, failed: 0 };
  for (const c of CATEGORIES) out[`cat:${c}`] = 0;
  const keys = Object.keys(out) as Filter[];
  for (const row of rows.values()) {
    for (const k of keys) if (matches(row, k)) out[k] += 1;
  }
  return out;
}

function sortValue(row: Row, key: SortKey): string | number {
  switch (key) {
    case "filename":
      return row.filename.toLowerCase();
    case "size":
      return row.size ?? -1;
    case "progress":
      return row.size ? row.bytes / row.size : -1;
    case "speed_bps":
      return row.speed_bps;
    case "state":
      return row.state;
    case "created_at":
      return row.created_at;
  }
}

export function visible(rows: Map<string, Row>, filter: Filter, key: SortKey, dir: SortDir): Row[] {
  const sign = dir === "asc" ? 1 : -1;
  return [...rows.values()]
    .filter((r) => matches(r, filter))
    .sort((a, b) => {
      const x = sortValue(a, key);
      const y = sortValue(b, key);
      if (x < y) return -sign;
      if (x > y) return sign;
      return a.id < b.id ? -1 : 1;
    });
}

export function canPause(row: Row): boolean {
  return isActive(row.state) || row.state === "queued";
}

export function canResume(row: Row): boolean {
  return row.state === "paused" || row.state === "failed";
}

export function canCancel(row: Row): boolean {
  return !DONE.includes(row.state);
}

export interface Segment {
  idx: number;
  widthPct: number;
  fillPct: number;
  state: ChunkState;
}

/** Chunk-map geometry, or null when it can't be drawn (unknown size, no chunks). */
export function segments(chunks: ChunkView[], total: number | null): Segment[] | null {
  if (!total || chunks.length === 0 || chunks.some((c) => c.end === null)) return null;
  return [...chunks]
    .sort((a, b) => a.start - b.start)
    .map((c) => {
      const len = (c.end as number) - c.start + 1;
      return {
        idx: c.idx,
        widthPct: (len / total) * 100,
        fillPct: len > 0 ? Math.min(100, (c.written / len) * 100) : 100,
        state: c.state,
      };
    });
}

export interface ParsedUrls {
  valid: string[];
  rejected: string[];
}

/** Same rule as the Rust side: one http(s) URL per line, duplicates once. */
export function parseUrls(text: string): ParsedUrls {
  const out: ParsedUrls = { valid: [], rejected: [] };
  for (const line of text.split(/\r?\n/).map((l) => l.trim()).filter(Boolean)) {
    const ok = /^https?:\/\/[^/]/i.test(line);
    const bucket = ok ? out.valid : out.rejected;
    if (!bucket.includes(line)) bucket.push(line);
  }
  return out;
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd app && pnpm test`
Expected: all tests in `format.test.ts` and `model.test.ts` pass.

- [ ] **Step 7: Implement `api.ts`**

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AddArgs, BatchResult, Detail, Download, Settings, Suggestion, UiEvent } from "./types";

export const listDownloads = () => invoke<Download[]>("list_downloads");
export const getDetail = (id: string) => invoke<Detail | null>("get_detail", { id });
export const addDownload = (args: AddArgs) => invoke<string>("add_download", { args });
export const addBatch = (text: string, saveDir: string | null) =>
  invoke<BatchResult>("add_batch", { text, saveDir });
export const suggest = (url: string) => invoke<Suggestion>("suggest", { url });
export const pause = (ids: string[]) => invoke<void>("pause", { ids });
export const resume = (ids: string[]) => invoke<void>("resume", { ids });
export const cancel = (ids: string[], deleteFile: boolean) => invoke<void>("cancel", { ids, deleteFile });
export const remove = (ids: string[], deleteFile: boolean) => invoke<void>("remove", { ids, deleteFile });
export const pauseAll = () => invoke<void>("pause_all");
export const resumeAll = () => invoke<void>("resume_all");
export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<Settings>("save_settings", { settings });
export const openFile = (id: string) => invoke<void>("open_file", { id });
export const openFolder = (id: string) => invoke<void>("open_folder", { id });
export const pickFolder = (start: string | null) => invoke<string | null>("pick_folder", { start });

export function onEvent(cb: (ev: UiEvent) => void): Promise<UnlistenFn> {
  return listen<UiEvent>("odm://event", (e) => cb(e.payload));
}

/** Commands reject with a plain string; anything else gets a generic message. */
export function errorText(e: unknown): string {
  return typeof e === "string" ? e : "Something went wrong. Check the log for details.";
}
```

- [ ] **Step 8: Implement `downloads.svelte.ts`**

```ts
import * as api from "./api";
import { applyEvent, counts, replaceAll, visible, type Filter, type Row, type SortDir, type SortKey } from "./model";
import type { UiEvent } from "./types";

class DownloadsStore {
  rows = $state<Map<string, Row>>(new Map());
  filter = $state<Filter>("all");
  sortKey = $state<SortKey>("created_at");
  sortDir = $state<SortDir>("desc");
  selected = $state<Set<string>>(new Set());
  error = $state<string | null>(null);

  visible = $derived(visible(this.rows, this.filter, this.sortKey, this.sortDir));
  counts = $derived(counts(this.rows));
  selectedRows = $derived([...this.selected].map((id) => this.rows.get(id)).filter((r): r is Row => !!r));

  /** Load the list, then follow live events. Call once at start-up. */
  async init(): Promise<void> {
    await api.onEvent((ev) => this.handle(ev));
    await this.reload();
  }

  async reload(): Promise<void> {
    try {
      this.rows = replaceAll(await api.listDownloads());
      this.pruneSelection();
    } catch (e) {
      this.error = api.errorText(e);
    }
  }

  handle(ev: UiEvent): void {
    const r = applyEvent(this.rows, ev);
    this.rows = r.rows;
    if (r.resync) void this.reload();
    if (ev.type === "removed") this.pruneSelection();
  }

  private pruneSelection(): void {
    const kept = [...this.selected].filter((id) => this.rows.has(id));
    if (kept.length !== this.selected.size) this.selected = new Set(kept);
  }

  /** Plain click selects one; ctrl toggles; shift extends from the last selected row. */
  select(id: string, mode: "one" | "toggle" | "range"): void {
    if (mode === "one") {
      this.selected = new Set([id]);
      return;
    }
    const next = new Set(this.selected);
    if (mode === "toggle") {
      if (next.has(id)) next.delete(id);
      else next.add(id);
    } else {
      const ids = this.visible.map((r) => r.id);
      const anchor = [...this.selected].at(-1);
      const a = anchor ? ids.indexOf(anchor) : -1;
      const b = ids.indexOf(id);
      if (a === -1 || b === -1) next.add(id);
      else for (const x of ids.slice(Math.min(a, b), Math.max(a, b) + 1)) next.add(x);
    }
    this.selected = next;
  }

  toggleSort(key: SortKey): void {
    if (this.sortKey === key) this.sortDir = this.sortDir === "asc" ? "desc" : "asc";
    else {
      this.sortKey = key;
      this.sortDir = key === "created_at" ? "desc" : "asc";
    }
  }

  /** Run a command and surface its error message in the UI. */
  async run(action: () => Promise<unknown>): Promise<boolean> {
    try {
      await action();
      this.error = null;
      return true;
    } catch (e) {
      this.error = api.errorText(e);
      return false;
    }
  }
}

export const downloads = new DownloadsStore();
```

- [ ] **Step 9: Type-check and commit**

Run: `cd app && pnpm test && pnpm check`
Expected: tests pass; `svelte-check` reports 0 errors.
```bash
git add app/src/lib
git commit -m "feat(ui): types, formatting, list model, API wrappers and store"
```

---

### Task 11: Main window (sidebar, toolbar, download table)

**Files:**
- Create: `app/src/components/Sidebar.svelte`, `Toolbar.svelte`, `ProgressBar.svelte`, `DownloadTable.svelte`
- Modify: `app/src/App.svelte`

**Interfaces:**
- Consumes: `downloads` store, `model.ts` helpers, `format.ts`, `api.ts`.
- Produces:
  - `<Sidebar />`: no props.
  - `<Toolbar onadd onbatch onsettings onremove />`: four callback props (`() => void`).
  - `<ProgressBar value={number | null} state={DownloadState} />`: `value` is 0–100, or `null` for unknown size.
  - `<DownloadTable onopen={(id: string) => void} />`.
  - `App.svelte` owns which dialog or panel is open through these state variables, which Tasks 12–14 fill in: `detailId: string | null`, `addOpen`, `batchOpen`, `settingsOpen`, `removeOpen` (all booleans).

- [ ] **Step 1: `Sidebar.svelte`**

```svelte
<script lang="ts">
  import { downloads } from "../lib/downloads.svelte";
  import { CATEGORIES, type Filter } from "../lib/model";

  const status: { key: Filter; label: string }[] = [
    { key: "all", label: "All downloads" },
    { key: "active", label: "Active" },
    { key: "queued", label: "Queued" },
    { key: "completed", label: "Completed" },
    { key: "failed", label: "Failed" },
  ];

  const categoryLabel: Record<string, string> = {
    video: "Video",
    audio: "Audio",
    document: "Documents",
    compressed: "Compressed",
    image: "Images",
    program: "Programs",
    general: "Other",
  };
</script>

<nav aria-label="Filters">
  {#each status as s (s.key)}
    <button class:active={downloads.filter === s.key} onclick={() => (downloads.filter = s.key)}>
      <span>{s.label}</span>
      <span class="count">{downloads.counts[s.key] ?? 0}</span>
    </button>
  {/each}
  <h2>Categories</h2>
  {#each CATEGORIES as c (c)}
    {@const key = `cat:${c}` as Filter}
    <button class:active={downloads.filter === key} onclick={() => (downloads.filter = key)}>
      <span>{categoryLabel[c]}</span>
      <span class="count">{downloads.counts[key] ?? 0}</span>
    </button>
  {/each}
</nav>

<style>
  nav {
    width: 200px;
    flex: none;
    padding: 8px;
    background: var(--surface);
    border-right: 1px solid var(--border);
    overflow-y: auto;
  }
  h2 {
    margin: 14px 8px 6px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  button {
    display: flex;
    justify-content: space-between;
    width: 100%;
    margin-bottom: 2px;
    border-color: transparent;
    background: transparent;
    text-align: left;
  }
  button.active {
    background: var(--surface-2);
    border-color: var(--border);
    color: var(--accent-light);
  }
  .count {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
</style>
```

- [ ] **Step 2: `Toolbar.svelte`**

```svelte
<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { canCancel, canPause, canResume } from "../lib/model";

  let {
    onadd,
    onbatch,
    onsettings,
    onremove,
  }: { onadd: () => void; onbatch: () => void; onsettings: () => void; onremove: () => void } = $props();

  const rows = $derived(downloads.selectedRows);
  const ids = (ok: (r: (typeof rows)[number]) => boolean) => rows.filter(ok).map((r) => r.id);
</script>

<div class="toolbar" role="toolbar" aria-label="Download actions">
  <button class="primary" onclick={onadd}>Add download</button>
  <button onclick={onbatch}>Add several</button>
  <span class="gap"></span>
  <button disabled={!rows.some(canResume)} onclick={() => downloads.run(() => api.resume(ids(canResume)))}>
    Resume
  </button>
  <button disabled={!rows.some(canPause)} onclick={() => downloads.run(() => api.pause(ids(canPause)))}>
    Pause
  </button>
  <button disabled={!rows.some(canCancel)} onclick={() => downloads.run(() => api.cancel(ids(canCancel), false))}>
    Cancel
  </button>
  <button disabled={rows.length === 0} onclick={onremove}>Remove</button>
  <span class="spacer"></span>
  <button onclick={onsettings}>Settings</button>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .gap {
    width: 10px;
  }
  .spacer {
    flex: 1;
  }
</style>
```

- [ ] **Step 3: `ProgressBar.svelte`**

```svelte
<script lang="ts">
  import type { DownloadState } from "../lib/types";

  let { value, state }: { value: number | null; state: DownloadState } = $props();

  const running = $derived(state === "downloading" || state === "probing" || state === "merging");
</script>

<div
  class="bar"
  class:error={state === "failed"}
  class:done={state === "completed"}
  class:indeterminate={value === null && running}
  role="progressbar"
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={value === null ? undefined : Math.round(value)}
>
  <div class="fill" style:width={value === null ? (state === "completed" ? "100%" : "0%") : `${value}%`}></div>
</div>

<style>
  .bar {
    height: 6px;
    background: var(--surface-2);
    border-radius: 3px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 200ms linear;
  }
  .done .fill {
    background: var(--success);
  }
  .error .fill {
    background: var(--error);
  }
  .indeterminate .fill {
    width: 35% !important;
    animation: slide 1.2s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(290%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .fill {
      transition: none;
    }
    .indeterminate .fill {
      animation: none;
      width: 100% !important;
      opacity: 0.4;
    }
  }
</style>
```

- [ ] **Step 4: `DownloadTable.svelte`**

```svelte
<script lang="ts">
  import { downloads } from "../lib/downloads.svelte";
  import { formatBytes, formatDate, formatEta, formatSpeed, percent, stateLabel } from "../lib/format";
  import type { SortKey } from "../lib/model";
  import ProgressBar from "./ProgressBar.svelte";

  let { onopen }: { onopen: (id: string) => void } = $props();

  const columns: { key: SortKey | null; label: string; cls: string }[] = [
    { key: "filename", label: "Name", cls: "name" },
    { key: "size", label: "Size", cls: "num" },
    { key: "progress", label: "Progress", cls: "progress" },
    { key: "speed_bps", label: "Speed", cls: "num" },
    { key: null, label: "Time left", cls: "num" },
    { key: "state", label: "Status", cls: "status" },
    { key: "created_at", label: "Added", cls: "num" },
  ];

  function click(e: MouseEvent, id: string) {
    downloads.select(id, e.shiftKey ? "range" : e.ctrlKey || e.metaKey ? "toggle" : "one");
  }

  function key(e: KeyboardEvent, id: string) {
    if (e.key === "Enter") onopen(id);
    if (e.key === " ") {
      e.preventDefault();
      downloads.select(id, "toggle");
    }
  }
</script>

<div class="wrap">
  {#if downloads.visible.length === 0}
    <p class="empty">
      {downloads.rows.size === 0 ? "No downloads yet. Choose Add download to start one." : "Nothing matches this filter."}
    </p>
  {:else}
    <table>
      <thead>
        <tr>
          {#each columns as c (c.label)}
            <th class={c.cls} aria-sort={c.key === downloads.sortKey ? (downloads.sortDir === "asc" ? "ascending" : "descending") : undefined}>
              {#if c.key}
                {@const k = c.key}
                <button onclick={() => downloads.toggleSort(k)}>
                  {c.label}{downloads.sortKey === k ? (downloads.sortDir === "asc" ? " ↑" : " ↓") : ""}
                </button>
              {:else}
                {c.label}
              {/if}
            </th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each downloads.visible as row (row.id)}
          {@const pct = percent(row.bytes, row.size)}
          <tr
            class:selected={downloads.selected.has(row.id)}
            tabindex="0"
            aria-selected={downloads.selected.has(row.id)}
            onclick={(e) => click(e, row.id)}
            ondblclick={() => onopen(row.id)}
            onkeydown={(e) => key(e, row.id)}
          >
            <td class="name" title={row.filename}>{row.filename}</td>
            <td class="num">{row.size === null ? "Unknown" : formatBytes(row.size)}</td>
            <td class="progress">
              <ProgressBar value={pct} state={row.state} />
              <span>{pct === null ? formatBytes(row.bytes) : `${pct.toFixed(pct < 100 ? 1 : 0)}%`}</span>
            </td>
            <td class="num">{row.state === "downloading" ? formatSpeed(row.speed_bps) : "—"}</td>
            <td class="num">{row.state === "downloading" ? formatEta(row.eta_s) : "—"}</td>
            <td class="status" class:bad={row.state === "failed"} title={row.error_message ?? ""}>
              {stateLabel(row.state)}
            </td>
            <td class="num">{formatDate(row.created_at)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .wrap {
    flex: 1;
    overflow: auto;
  }
  .empty {
    margin: 80px auto;
    text-align: center;
    color: var(--muted);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }
  th {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 0;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font-weight: 600;
    text-align: left;
  }
  th button {
    width: 100%;
    padding: 8px;
    border: none;
    border-radius: 0;
    background: none;
    text-align: inherit;
    color: inherit;
  }
  th:not(:has(button)) {
    padding: 8px;
  }
  td {
    padding: 7px 8px;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  tr.selected td {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  tbody tr:hover td {
    background: var(--surface-2);
  }
  tbody tr.selected:hover td {
    background: color-mix(in srgb, var(--accent) 24%, transparent);
  }
  tbody tr:focus-visible {
    outline: 2px solid var(--accent-light);
    outline-offset: -2px;
  }
  .name {
    width: auto;
  }
  .num {
    width: 96px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .status {
    width: 110px;
  }
  .status.bad {
    color: var(--error);
  }
  .progress {
    width: 190px;
  }
  td.progress {
    display: grid;
    grid-template-columns: 1fr 52px;
    align-items: center;
    gap: 8px;
  }
  td.progress span {
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }
</style>
```

- [ ] **Step 5: `App.svelte`**

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import DownloadTable from "./components/DownloadTable.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import Toolbar from "./components/Toolbar.svelte";
  import { downloads } from "./lib/downloads.svelte";

  let detailId = $state<string | null>(null);
  let addOpen = $state(false);
  let batchOpen = $state(false);
  let settingsOpen = $state(false);
  let removeOpen = $state(false);

  onMount(() => {
    void downloads.init();
  });

  function onkeydown(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
    if (typing) return;
    if (e.key === "Delete" && downloads.selected.size > 0) removeOpen = true;
    if (e.key === "Escape") detailId = null;
    if (e.ctrlKey && e.key.toLowerCase() === "n") {
      e.preventDefault();
      if (e.shiftKey) batchOpen = true;
      else addOpen = true;
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="app">
  <Toolbar
    onadd={() => (addOpen = true)}
    onbatch={() => (batchOpen = true)}
    onsettings={() => (settingsOpen = true)}
    onremove={() => (removeOpen = true)}
  />
  {#if downloads.error}
    <div class="error" role="alert">
      <span>{downloads.error}</span>
      <button onclick={() => (downloads.error = null)}>Dismiss</button>
    </div>
  {/if}
  <div class="body">
    <Sidebar />
    <DownloadTable onopen={(id) => (detailId = id)} />
    <!-- Task 12: <DetailPanel id={detailId} onclose={() => (detailId = null)} /> -->
  </div>
</div>

<!-- Tasks 13 and 14 add: AddDialog, BatchDialog, RemoveDialog, SettingsDialog -->

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 10px;
    background: color-mix(in srgb, var(--error) 18%, var(--bg));
    border-bottom: 1px solid var(--error);
    user-select: text;
  }
</style>
```
The two HTML comments are markers for Tasks 12–14 to replace. `detailId`, `addOpen`, `batchOpen`, `settingsOpen` and `removeOpen` are set here and read there; if `svelte-check` flags them as unused before those tasks land, that is expected only for this commit and must be gone by Task 14.

- [ ] **Step 6: Verify**

Run: `cd app && pnpm check && pnpm test && pnpm build`
Expected: 0 type errors (unused-state warnings for the five dialog flags are acceptable in this commit only), tests pass, the build succeeds.

Then `pnpm tauri dev` and check by eye (if you can't drive a GUI, say so in the report):
1. Toolbar across the top, sidebar on the left with counts all at 0, and the empty message in the middle.
2. Resume, Pause, Cancel and Remove are disabled with nothing selected.

- [ ] **Step 7: Commit**

```bash
git add app/src
git commit -m "feat(ui): main window with sidebar, toolbar and download table"
```

---

### Task 12: Detail panel and chunk map

**Files:**
- Create: `app/src/components/ChunkMap.svelte`, `app/src/components/DetailPanel.svelte`
- Modify: `app/src/App.svelte` (replace the Task 12 comment with the panel)

**Interfaces:**
- Consumes: `api.getDetail`, `api.openFile`, `api.openFolder`, `downloads.rows`, `segments()`.
- Produces:
  - `<ChunkMap chunks={ChunkView[]} total={number | null} />`
  - `<DetailPanel id={string | null} onclose={() => void} />`: renders nothing when `id` is null.

- [ ] **Step 1: `ChunkMap.svelte`**

```svelte
<script lang="ts">
  import { segments } from "../lib/model";
  import type { ChunkView } from "../lib/types";

  let { chunks, total }: { chunks: ChunkView[]; total: number | null } = $props();

  const segs = $derived(segments(chunks, total));
  const done = $derived(chunks.filter((c) => c.state === "done").length);
</script>

{#if segs}
  <div class="map" role="img" aria-label={`${done} of ${chunks.length} parts finished`}>
    {#each segs as s (s.idx)}
      <div class="seg {s.state}" style:width={`${s.widthPct}%`} title={`Part ${s.idx + 1}: ${s.fillPct.toFixed(0)}%`}>
        <div class="fill" style:width={`${s.fillPct}%`}></div>
      </div>
    {/each}
  </div>
  <p class="legend">{done} of {chunks.length} parts finished</p>
{:else}
  <p class="legend">The part map appears once the file size is known.</p>
{/if}

<style>
  .map {
    display: flex;
    gap: 2px;
    height: 18px;
  }
  .seg {
    min-width: 2px;
    background: var(--surface-2);
    border-radius: 2px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 200ms linear;
  }
  .seg.active .fill {
    background: var(--accent-light);
  }
  .seg.done .fill {
    background: var(--success);
  }
  .seg.retrying .fill {
    background: #f59e0b;
  }
  .seg.failed {
    outline: 1px solid var(--error);
  }
  .seg.failed .fill {
    background: var(--error);
  }
  .legend {
    margin: 6px 0 0;
    color: var(--muted);
  }
  @media (prefers-reduced-motion: reduce) {
    .fill {
      transition: none;
    }
  }
</style>
```

- [ ] **Step 2: `DetailPanel.svelte`**

```svelte
<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import { formatBytes, formatEta, formatSpeed, percent, stateLabel } from "../lib/format";
  import type { Detail } from "../lib/types";
  import ChunkMap from "./ChunkMap.svelte";

  let { id, onclose }: { id: string | null; onclose: () => void } = $props();

  let detail = $state<Detail | null>(null);
  let copied = $state(false);

  const row = $derived(id ? downloads.rows.get(id) : undefined);
  // Live chunks while it runs; the stored ones otherwise.
  const chunks = $derived(row?.chunks ?? detail?.chunks ?? []);
  const pct = $derived(row ? percent(row.bytes, row.size) : null);

  $effect(() => {
    const wanted = id;
    // Re-read when the download changes state, so stored chunks stay current.
    void row?.state;
    detail = null;
    if (!wanted) return;
    api
      .getDetail(wanted)
      .then((d) => {
        if (id === wanted) detail = d;
      })
      .catch((e) => (downloads.error = api.errorText(e)));
  });

  // The download was removed while its panel was open.
  $effect(() => {
    if (id && !row) onclose();
  });

  async function copyUrl() {
    if (!row) return;
    await navigator.clipboard.writeText(row.url);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

{#if id && row}
  <aside aria-label="Download details">
    <header>
      <h2 title={row.filename}>{row.filename}</h2>
      <button onclick={onclose} aria-label="Close details">Close</button>
    </header>

    <section>
      <ChunkMap {chunks} total={row.size} />
    </section>

    <dl>
      <dt>Status</dt>
      <dd class:bad={row.state === "failed"}>{stateLabel(row.state)}</dd>
      {#if row.error_message}
        <dt>Problem</dt>
        <dd class="bad wrap">{row.error_message}</dd>
      {/if}
      <dt>Downloaded</dt>
      <dd>
        {formatBytes(row.bytes)}{row.size === null ? "" : ` of ${formatBytes(row.size)}`}{pct === null ? "" : ` (${pct.toFixed(1)}%)`}
      </dd>
      <dt>Speed</dt>
      <dd>{row.state === "downloading" ? formatSpeed(row.speed_bps) : "—"}</dd>
      <dt>Time left</dt>
      <dd>{row.state === "downloading" ? formatEta(row.eta_s) : "—"}</dd>
      <dt>Saved to</dt>
      <dd class="wrap">{row.save_dir}</dd>
      <dt>Address</dt>
      <dd class="wrap">{row.url}</dd>
      {#if detail?.final_url && detail.final_url !== row.url}
        <dt>Redirects to</dt>
        <dd class="wrap">{detail.final_url}</dd>
      {/if}
      <dt>Connections</dt>
      <dd>Up to {row.max_connections}</dd>
      {#if detail && detail.header_names.length > 0}
        <dt>Sent headers</dt>
        <dd class="wrap">{detail.header_names.join(", ")}</dd>
      {/if}
    </dl>

    <footer>
      <button disabled={row.state !== "completed"} onclick={() => downloads.run(() => api.openFile(row.id))}>
        Open file
      </button>
      <button onclick={() => downloads.run(() => api.openFolder(row.id))}>Open folder</button>
      <button onclick={copyUrl}>{copied ? "Copied" : "Copy address"}</button>
    </footer>
  </aside>
{/if}

<style>
  aside {
    width: 360px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 12px;
    background: var(--surface);
    border-left: 1px solid var(--border);
    overflow-y: auto;
    user-select: text;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  h2 {
    margin: 0;
    font-size: 14px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  dl {
    display: grid;
    grid-template-columns: 96px 1fr;
    gap: 6px 10px;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    min-width: 0;
  }
  .wrap {
    overflow-wrap: anywhere;
  }
  .bad {
    color: var(--error);
  }
  footer {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: auto;
  }
</style>
```

- [ ] **Step 3: Mount it in `App.svelte`**

Add `import DetailPanel from "./components/DetailPanel.svelte";` and replace the Task 12 comment with:
```svelte
    <DetailPanel id={detailId} onclose={() => (detailId = null)} />
```

- [ ] **Step 4: Verify and commit**

Run: `cd app && pnpm check && pnpm test && pnpm build`
Expected: 0 errors, tests pass, build succeeds.
```bash
git add app/src
git commit -m "feat(ui): detail panel with live chunk map"
```

---

### Task 13: Dialogs for adding and removing downloads

**Files:**
- Create: `app/src/components/Modal.svelte`, `AddDialog.svelte`, `BatchDialog.svelte`, `RemoveDialog.svelte`
- Modify: `app/src/App.svelte` (mount the three dialogs)

**Interfaces:**
- Consumes: `api.addDownload`, `api.addBatch`, `api.suggest`, `api.pickFolder`, `api.getSettings`, `api.remove`, `parseUrls`, `CATEGORIES`, `downloads`.
- Produces:
  - `<Modal open title onclose width? >children</Modal>`: a native `<dialog>` opened with `showModal()`; Escape and the backdrop close it.
  - `<AddDialog open onclose />`, `<BatchDialog open onclose />`, `<RemoveDialog open onclose />`. Each takes `open: boolean` and `onclose: () => void`.

- [ ] **Step 1: `Modal.svelte`**

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    open,
    title,
    onclose,
    width = 520,
    children,
  }: { open: boolean; title: string; onclose: () => void; width?: number; children: Snippet } = $props();

  let el = $state<HTMLDialogElement>();

  $effect(() => {
    if (!el) return;
    if (open && !el.open) el.showModal();
    if (!open && el.open) el.close();
  });

  function backdrop(e: MouseEvent) {
    if (e.target === el) onclose();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={el} style:width={`${width}px`} aria-labelledby="modal-title" onclose={onclose} onclick={backdrop}>
  {#if open}
    <div class="content">
      <h2 id="modal-title">{title}</h2>
      {@render children()}
    </div>
  {/if}
</dialog>

<style>
  dialog {
    max-width: calc(100vw - 40px);
    padding: 0;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.6);
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.55);
  }
  .content {
    padding: 18px;
  }
  h2 {
    margin: 0 0 14px;
    font-size: 15px;
  }
</style>
```
Escape closes a modal `<dialog>` natively and fires `close`, which calls `onclose`. The keyboard a11y warning is silenced because the click handler only serves the backdrop; the keyboard path is Escape.

Shared form styles: add to `app/src/app.css`:
```css
.field {
  display: grid;
  gap: 4px;
  margin-bottom: 12px;
}
.field > span {
  color: var(--muted);
}
.field input,
.field select,
.field textarea {
  width: 100%;
}
.row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.row > input {
  flex: 1;
}
.hint {
  margin: 0;
  color: var(--muted);
}
.form-error {
  margin: 0 0 12px;
  color: var(--error);
  user-select: text;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 16px;
}
.check {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}
```

- [ ] **Step 2: `AddDialog.svelte`**

```svelte
<script lang="ts">
  import * as api from "../lib/api";
  import { formatBytes } from "../lib/format";
  import { CATEGORIES } from "../lib/model";
  import type { Suggestion } from "../lib/types";
  import Modal from "./Modal.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let url = $state("");
  let filename = $state("");
  let saveDir = $state("");
  let category = $state("");
  let connections = $state(8);
  let suggestion = $state<Suggestion | null>(null);
  let checking = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const validUrl = $derived(/^https?:\/\/[^/]/i.test(url.trim()));

  // Fresh form every time the dialog opens.
  $effect(() => {
    if (!open) return;
    url = "";
    filename = "";
    saveDir = "";
    category = "";
    suggestion = null;
    error = null;
    api
      .getSettings()
      .then((s) => (connections = s.max_connections))
      .catch(() => {});
  });

  // Ask the server about the address once typing pauses.
  $effect(() => {
    const wanted = url.trim();
    suggestion = null;
    if (!open || !validUrl) return;
    const timer = setTimeout(() => {
      checking = true;
      api
        .suggest(wanted)
        .then((s) => {
          if (url.trim() === wanted) suggestion = s;
        })
        .catch(() => {})
        .finally(() => (checking = false));
    }, 500);
    return () => clearTimeout(timer);
  });

  async function browse() {
    const picked = await api.pickFolder(saveDir || null).catch(() => null);
    if (picked) saveDir = picked;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!validUrl || busy) return;
    busy = true;
    error = null;
    try {
      await api.addDownload({
        url: url.trim(),
        filename: filename.trim() || null,
        save_dir: saveDir.trim() || null,
        category: category || null,
        connections,
      });
      onclose();
    } catch (err) {
      error = api.errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {open} title="Add download" {onclose}>
  <form onsubmit={submit}>
    <label class="field">
      <span>Address</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input type="url" bind:value={url} placeholder="https://example.com/file.zip" autofocus required />
    </label>
    <p class="hint">
      {#if checking}
        Checking the address…
      {:else if suggestion}
        {suggestion.size === null ? "Size unknown" : formatBytes(suggestion.size)} ·
        {suggestion.resumable ? "can be paused and resumed" : "cannot be resumed if interrupted"}
      {:else if url && !validUrl}
        Enter an address that starts with http:// or https://
      {:else}
        &nbsp;
      {/if}
    </p>

    <label class="field">
      <span>File name</span>
      <input bind:value={filename} placeholder={suggestion?.filename ?? "Chosen automatically"} />
    </label>

    <label class="field">
      <span>Save to</span>
      <span class="row">
        <input bind:value={saveDir} placeholder="Your download folder, sorted by type" />
        <button type="button" onclick={browse}>Browse</button>
      </span>
    </label>

    <div class="row">
      <label class="field" style="flex:1">
        <span>Type</span>
        <select bind:value={category}>
          <option value="">Automatic{suggestion ? ` (${suggestion.category})` : ""}</option>
          {#each CATEGORIES as c (c)}
            <option value={c}>{c}</option>
          {/each}
        </select>
      </label>
      <label class="field" style="width:140px">
        <span>Connections</span>
        <input type="number" min="1" max="32" bind:value={connections} />
      </label>
    </div>

    {#if error}<p class="form-error" role="alert">{error}</p>{/if}

    <div class="actions">
      <button type="button" onclick={onclose}>Cancel</button>
      <button type="submit" class="primary" disabled={!validUrl || busy}>Start download</button>
    </div>
  </form>
</Modal>
```

- [ ] **Step 3: `BatchDialog.svelte`**

```svelte
<script lang="ts">
  import * as api from "../lib/api";
  import { parseUrls } from "../lib/model";
  import Modal from "./Modal.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let text = $state("");
  let saveDir = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let rejected = $state<string[]>([]);

  const parsed = $derived(parseUrls(text));

  $effect(() => {
    if (!open) return;
    text = "";
    saveDir = "";
    error = null;
    rejected = [];
  });

  async function browse() {
    const picked = await api.pickFolder(saveDir || null).catch(() => null);
    if (picked) saveDir = picked;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (parsed.valid.length === 0 || busy) return;
    busy = true;
    error = null;
    try {
      const result = await api.addBatch(text, saveDir.trim() || null);
      if (result.rejected.length === 0) {
        onclose();
      } else {
        // Keep the dialog open so the lines that were not added can be fixed.
        rejected = result.rejected;
        text = result.rejected.join("\n");
      }
    } catch (err) {
      error = api.errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {open} title="Add several downloads" {onclose} width={600}>
  <form onsubmit={submit}>
    <label class="field">
      <span>Addresses, one per line</span>
      <!-- svelte-ignore a11y_autofocus -->
      <textarea bind:value={text} rows="10" spellcheck="false" autofocus placeholder="https://example.com/one.zip&#10;https://example.com/two.zip"></textarea>
    </label>
    <p class="hint">
      {parsed.valid.length} ready to add{parsed.rejected.length > 0
        ? ` · ${parsed.rejected.length} not a web address`
        : ""}
    </p>

    <label class="field" style="margin-top:12px">
      <span>Save to</span>
      <span class="row">
        <input bind:value={saveDir} placeholder="Your download folder, sorted by type" />
        <button type="button" onclick={browse}>Browse</button>
      </span>
    </label>

    {#if rejected.length > 0}
      <p class="form-error" role="alert">
        {rejected.length} {rejected.length === 1 ? "line was" : "lines were"} not added. They are still in the box above.
      </p>
    {/if}
    {#if error}<p class="form-error" role="alert">{error}</p>{/if}

    <div class="actions">
      <button type="button" onclick={onclose}>Cancel</button>
      <button type="submit" class="primary" disabled={parsed.valid.length === 0 || busy}>
        Add {parsed.valid.length || ""}
      </button>
    </div>
  </form>
</Modal>
```

- [ ] **Step 4: `RemoveDialog.svelte`**

```svelte
<script lang="ts">
  import * as api from "../lib/api";
  import { downloads } from "../lib/downloads.svelte";
  import Modal from "./Modal.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let deleteFile = $state(false);
  const count = $derived(downloads.selected.size);

  $effect(() => {
    if (open) deleteFile = false;
  });

  async function confirm() {
    const ids = [...downloads.selected];
    if (await downloads.run(() => api.remove(ids, deleteFile))) downloads.selected = new Set();
    onclose();
  }
</script>

<Modal {open} title={count === 1 ? "Remove this download?" : `Remove ${count} downloads?`} {onclose} width={420}>
  <p class="hint">They leave the list. Downloads still running are stopped first.</p>
  <label class="check" style="margin-top:12px">
    <input type="checkbox" bind:checked={deleteFile} />
    <span>Also delete the files from disk</span>
  </label>
  <div class="actions">
    <button onclick={onclose}>Keep</button>
    <button class="primary" onclick={confirm} disabled={count === 0}>Remove</button>
  </div>
</Modal>
```

- [ ] **Step 5: Mount them in `App.svelte`**

Import the three components and replace the "Tasks 13 and 14" comment with:
```svelte
<AddDialog open={addOpen} onclose={() => (addOpen = false)} />
<BatchDialog open={batchOpen} onclose={() => (batchOpen = false)} />
<RemoveDialog open={removeOpen} onclose={() => (removeOpen = false)} />
<!-- Task 14: SettingsDialog -->
```

- [ ] **Step 6: Verify and commit**

Run: `cd app && pnpm check && pnpm test && pnpm build`
Expected: 0 errors, tests pass, build succeeds.

Then `pnpm tauri dev` and check by eye (say so in the report if you can't drive a GUI):
1. Add download → paste `https://proof.ovh.net/files/10Mb.dat` → the hint line shows a size and "can be paused and resumed" → Start download → a row appears and completes.
2. Select the row → Remove → tick "Also delete the files from disk" → Remove: the row and the file are gone.

```bash
git add app/src
git commit -m "feat(ui): add, add-several and remove dialogs"
```

---

### Task 14: Settings dialog

**Files:**
- Create: `app/src/components/SettingsDialog.svelte`
- Modify: `app/src/App.svelte` (mount it)

**Interfaces:**
- Consumes: `api.getSettings`, `api.saveSettings`, `api.pickFolder`, `CATEGORIES`.
- Produces: `<SettingsDialog open onclose />`. Saving sends the whole `Settings` object; the command returns the clamped version, which becomes the form's state.

- [ ] **Step 1: `SettingsDialog.svelte`**

```svelte
<script lang="ts">
  import * as api from "../lib/api";
  import { CATEGORIES } from "../lib/model";
  import type { Settings } from "../lib/types";
  import Modal from "./Modal.svelte";

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();

  let s = $state<Settings | null>(null);
  let skipHosts = $state("");
  let minCaptureMb = $state(1);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const MB = 1024 * 1024;

  $effect(() => {
    if (!open) return;
    s = null;
    error = null;
    api
      .getSettings()
      .then((loaded) => {
        s = loaded;
        skipHosts = loaded.skip_hosts.join("\n");
        minCaptureMb = Math.round((loaded.min_capture_bytes / MB) * 10) / 10;
      })
      .catch((e) => (error = api.errorText(e)));
  });

  async function browse() {
    if (!s) return;
    const picked = await api.pickFolder(s.download_dir || null).catch(() => null);
    if (picked) s.download_dir = picked;
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!s || busy) return;
    busy = true;
    error = null;
    try {
      await api.saveSettings({
        ...s,
        skip_hosts: skipHosts
          .split(/\r?\n/)
          .map((h) => h.trim())
          .filter(Boolean),
        min_capture_bytes: Math.max(0, Math.round(minCaptureMb * MB)),
      });
      onclose();
    } catch (err) {
      error = api.errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {open} title="Settings" {onclose} width={620}>
  {#if s}
    <form onsubmit={save}>
      <div class="scroll">
        <h3>Folders</h3>
        <label class="field">
          <span>Download folder</span>
          <span class="row">
            <input bind:value={s.download_dir} required />
            <button type="button" onclick={browse}>Browse</button>
          </span>
        </label>
        <p class="hint">Each type goes into its own folder inside the download folder. Leave one empty to save that type directly in it.</p>
        <div class="grid">
          {#each CATEGORIES as c (c)}
            <label class="field">
              <span>{c}</span>
              <input bind:value={s.category_folders[c]} />
            </label>
          {/each}
        </div>

        <h3>Downloads</h3>
        <div class="grid">
          <label class="field">
            <span>Downloads at once (1–10)</span>
            <input type="number" min="1" max="10" bind:value={s.max_concurrent} />
          </label>
          <label class="field">
            <span>Connections per download (1–32)</span>
            <input type="number" min="1" max="32" bind:value={s.max_connections} />
          </label>
          <label class="field">
            <span>Speed limit in KB/s (0 = no limit)</span>
            <input type="number" min="0" bind:value={s.speed_limit_kbps} />
          </label>
          <label class="field">
            <span>Tries per part before giving up (1–20)</span>
            <input type="number" min="1" max="20" bind:value={s.max_attempts} />
          </label>
          <label class="field">
            <span>Connect timeout in seconds (5–120)</span>
            <input type="number" min="5" max="120" bind:value={s.connect_timeout_s} />
          </label>
          <label class="field">
            <span>Read timeout in seconds (10–300)</span>
            <input type="number" min="10" max="300" bind:value={s.read_timeout_s} />
          </label>
        </div>
        <label class="field">
          <span>FFmpeg location (used for video streams; leave empty to find it automatically)</span>
          <input bind:value={s.ffmpeg_path} />
        </label>

        <h3>Window</h3>
        <label class="check"><input type="checkbox" bind:checked={s.close_to_tray} /><span>Keep running in the tray when the window is closed</span></label>
        <label class="check"><input type="checkbox" bind:checked={s.start_minimized} /><span>Start hidden in the tray</span></label>
        <label class="check"><input type="checkbox" bind:checked={s.notifications} /><span>Show a notification when a download finishes</span></label>

        <h3>Browser</h3>
        <p class="hint">These apply once the ODM browser extension is installed.</p>
        <label class="check"><input type="checkbox" bind:checked={s.capture_enabled} /><span>Take over downloads started in the browser</span></label>
        <label class="field">
          <span>Leave files smaller than this many MB to the browser</span>
          <input type="number" min="0" step="0.1" bind:value={minCaptureMb} />
        </label>
        <label class="field">
          <span>Sites to leave to the browser, one per line</span>
          <textarea rows="3" spellcheck="false" bind:value={skipHosts}></textarea>
        </label>
      </div>

      {#if error}<p class="form-error" role="alert">{error}</p>{/if}
      <div class="actions">
        <button type="button" onclick={onclose}>Cancel</button>
        <button type="submit" class="primary" disabled={busy}>Save</button>
      </div>
    </form>
  {:else if error}
    <p class="form-error" role="alert">{error}</p>
    <div class="actions"><button onclick={onclose}>Close</button></div>
  {:else}
    <p class="hint">Loading…</p>
  {/if}
</Modal>

<style>
  .scroll {
    max-height: 62vh;
    overflow-y: auto;
    padding-right: 6px;
  }
  h3 {
    margin: 16px 0 8px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  h3:first-child {
    margin-top: 0;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0 12px;
  }
  .hint {
    margin-bottom: 8px;
  }
</style>
```

- [ ] **Step 2: Mount it in `App.svelte`**

Import `SettingsDialog` and replace the Task 14 comment with:
```svelte
<SettingsDialog open={settingsOpen} onclose={() => (settingsOpen = false)} />
```

- [ ] **Step 3: Verify and commit**

Run: `cd app && pnpm check && pnpm test && pnpm build`
Expected: 0 errors and **0 warnings** from `svelte-check` (every dialog flag in `App.svelte` is now used), tests pass, build succeeds.

Then `pnpm tauri dev` and check by eye (say so in the report if you can't drive a GUI):
1. Settings opens with the current values. Set "Connections per download" to 99 and Save; reopen: it shows 32.
2. `%APPDATA%\ODM\settings.json` (or `$ODM_DATA_DIR/settings.json`) contains `"max_connections": 32`.

```bash
git add app/src
git commit -m "feat(ui): settings dialog"
```

---

### Task 15: Whole-app verification and README

**Files:**
- Create: `README.md`

- [ ] **Step 1: Everything green**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -- --test-threads=4
cd app && pnpm check && pnpm test && pnpm build && cd ..
```
Expected: no formatting differences, no clippy warnings, all Rust tests pass, 0 `svelte-check` errors and warnings, all vitest tests pass, the front end builds.

- [ ] **Step 2: Build the app and run the end-to-end check**

```bash
cd app && pnpm tauri build --debug --no-bundle && cd ..
export ODM_DATA_DIR="$TEMP/odm-e2e"; rm -rf "$ODM_DATA_DIR"
./target/debug/odm-app.exe
```
With the window open, go through this list and record each result in the report. If you can't drive a GUI, say so and leave the list for the controller.

1. Add download → `https://proof.ovh.net/files/100Mb.dat` → Start download. The row shows a moving progress bar, a speed and a time left.
2. Double-click the row. The detail panel opens and the part map shows several segments filling at once.
3. Pause, then Resume. The percentage continues from where it stopped, not from 0.
4. Wait for it to finish. Status is "Completed", a notification appears, and Open folder shows a 12,500,000-byte file.
5. Add several → paste two valid addresses and the word `nope` → Add. Two rows appear; the dialog stays open with `nope` in the box and the "1 line was not added" message.
6. Close the window. It hides to the tray. Left-click the tray icon: it comes back.
7. Start a download, then tray → Quit. A "Quit ODM" question appears. Choose Quit. Start the app again: the download is in the list and continues.
8. Start the app a second time while it is running. No second window appears; the first one is focused.

- [ ] **Step 3: Write `README.md`**

```markdown
# ODM (Open Downloader Manager)

A personal download manager for Windows. It downloads files over several connections at once, resumes after a pause or a crash, and keeps a queue with speed and concurrency limits.

## Status

- Done: the download engine, the command-line tool and the desktop app.
- Not yet built: the browser extension, video streams (HLS) and the installer.

## Requirements

- Windows 10 or 11 with the WebView2 runtime (included in Windows 11)
- Rust (stable, MSVC toolchain) and the Visual Studio C++ Build Tools
- Node 20 or newer and pnpm

## Run the desktop app

    cd app
    pnpm install
    pnpm tauri dev

Build a standalone executable (written to `target/debug/odm-app.exe`):

    cd app
    pnpm tauri build --debug --no-bundle

## Command line

    cargo run -p odm-cli --release -- get <URL> [--dir DIR] [--name NAME] [-c CONNECTIONS] [--limit BYTES_PER_SECOND]

## Where ODM keeps its files

`%APPDATA%\ODM\` holds the database (`odm.db`), `settings.json` and `logs\`. Set the `ODM_DATA_DIR` environment variable to use another folder, for example to try a build without touching your real data.

## Tests

    cargo test --workspace -- --test-threads=4
    cd app && pnpm test && pnpm check

## Layout

| Path | What it is |
|---|---|
| `crates/odm-core` | Shared types: states, errors, file names, categories |
| `crates/odm-store` | SQLite storage |
| `crates/odm-engine` | The download engine and queue |
| `crates/odm-appcore` | App logic that doesn't depend on Tauri: settings, logging, UI data, event bridge |
| `crates/odm-cli` | Command-line tool |
| `crates/odm-testserver` | Test-only HTTP server that can misbehave on purpose |
| `app/src-tauri` | The Tauri shell: commands, tray, window |
| `app/src` | The Svelte user interface |
| `docs/superpowers` | Design spec and implementation plans |
```

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: README for the engine, CLI and desktop app"
```

---

## What this plan picks up from Plan 1's carry-over

| Deferred item | Where it is handled |
|---|---|
| Runtime `set_config` (no manager restart) | Task 2 |
| Read-only queries on the handle | Task 2 (`list`, `get`, `chunks`, `header_names`, `active_count`) |
| `add()` not atomic with its headers | Tasks 1–2 |
| A database from a newer version isn't rejected | Task 1 |
| `set_rate` doesn't wake sleepers | Task 2 |
| `EngineConfig` isn't validated | Task 2 (`sanitized`) |
| Category folders joined without checks | Task 3 (`Settings::clamp`) |
| Credentials in logged URLs | Task 4 (`redact_url`) |
| Subscribers must resync on `Lagged` | Task 6 (bridge) and Task 10 (store) |
| Category from the probe's MIME type | Task 2 |
| `settings.json` and rotating logs | Tasks 3–4 |

## Still deferred after this plan

These stay in `docs/superpowers/plans/2026-10-01-odm-plan-1-carryover.md` for Plan 3 or later:
- Content-Disposition parsing (`;` inside quotes, `filename*` charsets) and header-name normalization: Plan 3, where the browser sends headers.
- "Refresh address" for an expired link (`refresh_url`): Plan 3.
- The HLS quality picker in the Add dialog: Plan 4.
- A retry after a checksum mismatch re-checks the same bytes; rename retry when antivirus holds the file; a crash between the final rename and the Completed state re-downloads the file; a 416 without Content-Range is taken as an empty file; downloads still running at the shutdown deadline: reliability items for Plan 5.
- Clicking the completion notification to open the folder: not supported by the notification plugin on Windows (see Task 9).
- The detail panel does not show a per-part attempt count (spec section 9 lists "attempts"): the engine does not publish it yet. Add it to `ChunkView` when a later plan needs it.
