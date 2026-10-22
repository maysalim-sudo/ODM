# Torrent Downloads Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ODM downloads magnet links and `.torrent` files as ordinary downloads, with file choice at add time and while running, live tracking (peers, speeds, per-file progress, piece map), and no seeding.

**Architecture:** A new crate `odm-torrent` wraps librqbit 9.0.1 behind a `TorrentClient` trait (real `LibrqbitClient` + scriptable `FakeTorrentClient`). `odm-engine` gains kind `"torrent"` with `run_torrent` and a metadata fetcher outside the queue; `odm-store` migration V4 holds info hash, source and file list; appcore/app/extension/installer add intake and UI.

**Tech Stack:** Rust 1.98 (tokio, rusqlite, librqbit `=9.0.1`, async-trait), Tauri 2, Svelte 5 + vitest, TypeScript extension + vitest, NSIS.

**Spec:** `docs/superpowers/specs/2026-10-23-odm-torrents-design.md`

## Global Constraints

- Single branch `master`; commit after each task; **ask the user before any push**.
- librqbit pinned `=9.0.1`; only `crates/odm-torrent` depends on it.
- No seeding: on `finished`, pause → remove from session with `delete_files = false` → `Completed`.
- Listen port: random in 49152–65535, chosen on first run, checked free for TCP **and** UDP, a few retries.
- Defaults: UPnP on; upload limit while downloading 100 KB/s (0 = no upload, empty = unlimited); max peers per torrent 100.
- `.torrent` URL fetch: max 10 MB, 15 s timeout, captured headers and cookies, off the UI thread.
- Persistence under `%APPDATA%\ODM\torrents\`; stored `.torrent` copies in `%APPDATA%\ODM\torrents\files\<info_hash>.torrent`.
- `info_hash` is lowercase hex everywhere (store, DTO, logs).
- Torrents always show the confirm dialog, regardless of the browser-confirm setting.
- Copy: "Fetching file list…", "Checking files…", "Waiting for you", "Download", "Download all".
- Proxy: manual `socks5://`/`socks5h://` goes to librqbit `connect.proxy_url`; `http(s)://` proxies are not used for torrents and the card says "Torrents connect directly; only a SOCKS5 proxy can carry torrent traffic."
- Checks: `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cd app && pnpm test && pnpm check`, `cd extension && pnpm test && pnpm check && pnpm build`.
- Anything that registers with Windows (associations, installer) is verified by the user from their own shell, not by Claude (MSIX container).

## Review Focus

1. **Magnet that never gets metadata** (dead swarm): the row must stay "Fetching file list…" indefinitely, be removable, and removal must leave no librqbit handle → test `remove_during_metadata_drops_handle` in Task 6.
2. **App closed mid-download, reopened**: the torrent comes back Queued, resumes from fastresume without re-downloading, and librqbit does not start it on its own before the queue does → test `restart_resumes_without_redownload` in Task 3.
3. **User unticks every file / unticks while complete**: at least one file must stay selected (error "Keep at least one file selected."); ticking a new file on a Completed torrent re-queues it → tests `set_files_rejects_empty` and `retick_completed_requeues` in Task 7.
4. **Same torrent added twice from different sources** (magnet then `.torrent`, or magnet with extra trackers): one row, trackers merged, second add returns `existing: true` → test `duplicate_merges_trackers` in Task 6.
5. **Torrent file names Windows cannot create** (`CON`, `aux.txt`, `a:b`): download must succeed; display keeps the original path → test `reserved_names_download` in Task 3.

---

## File Structure

| Path | Responsibility |
|---|---|
| `crates/odm-torrent/Cargo.toml` | crate; features `fake` |
| `crates/odm-torrent/src/lib.rs` | `TorrentClient` trait, public types, re-exports |
| `crates/odm-torrent/src/types.rs` | `TorrentSource`, `InfoHash`, `ParsedTorrent`, `TorrentMeta`, `TorrentFile`, `TorrentStats`, `PieceMap`, `TorrentSettings`, `StartTorrent`, `TorrentError` |
| `crates/odm-torrent/src/parse.rs` | magnet / `.torrent` parsing without network |
| `crates/odm-torrent/src/port.rs` | TCP+UDP free-port pick |
| `crates/odm-torrent/src/rqbit.rs` | `LibrqbitClient` |
| `crates/odm-torrent/src/fake.rs` | `FakeTorrentClient` (feature `fake`) |
| `crates/odm-torrent/tests/rqbit.rs` | integration tests vs local seeder |
| `crates/odm-store/src/migrations.rs`, `repo.rs`, `models.rs`, `lib.rs` | V4 + torrent repo methods |
| `crates/odm-core/src/category.rs` | `Category::Torrent` |
| `crates/odm-core/src/state.rs` | `Completed → Queued` transition |
| `crates/odm-core/src/ipc.rs` | `ADD_TORRENT`, `AddTorrent` payload |
| `crates/odm-engine/src/torrent/mod.rs` | `TorrentRequest`, `TorrentAdded`, metadata fetcher, helpers |
| `crates/odm-engine/src/torrent/run.rs` | `run_torrent` |
| `crates/odm-engine/src/torrent/progress.rs` | monotonic progress tracker |
| `crates/odm-engine/src/manager.rs`, `config.rs`, `events.rs`, `lib.rs` | wiring |
| `crates/odm-appcore/src/torrent.rs` | source resolution (text / path / URL fetch) |
| `crates/odm-appcore/src/{api,ipc,dto,settings}.rs` | add/confirm/detail/settings/IPC |
| `app/src-tauri/src/{commands,lib}.rs` | commands, argv forwarding |
| `app/src/lib/fileTree.ts` (+ test) | tree building and selection logic |
| `app/src/components/{AddDialog,ConfirmDialog,DetailPanel,ChunkMap,SettingsPage}.svelte`, `TorrentFiles.svelte` | UI |
| `extension/src/lib/torrent.ts` (+ test), `background.ts`, `content.ts` | magnet/.torrent capture |
| `app/src-tauri/installer-hooks.nsh`, `tauri.conf.json` | associations |
| `docs/superpowers/plans/2026-10-23-odm-torrents-smoke.md` | manual checklist |

---

### Task 1: `odm-torrent` crate — types, trait, parsing

**Files:**
- Create: `crates/odm-torrent/Cargo.toml`, `src/lib.rs`, `src/types.rs`, `src/parse.rs`
- Modify: root `Cargo.toml` (workspace deps `odm-torrent = { path = "crates/odm-torrent" }`, `librqbit = { version = "=9.0.1", default-features = false, features = ["rust-tls"] }` (rustls, matching ODM's reqwest), `async-trait = "0.1"`)

**Interfaces:**
- Produces:
  ```rust
  pub enum TorrentSource { Magnet(String), Bytes(bytes::Bytes) }
  #[derive(Clone, PartialEq, Eq, Hash, Debug)] pub struct InfoHash(String); // lowercase hex, 40 chars
  impl InfoHash { pub fn parse(hex: &str) -> Option<Self>; pub fn as_str(&self) -> &str }
  pub struct TorrentFile { pub idx: usize, pub path: String, pub size: u64 }   // path uses '/'
  pub struct TorrentMeta { pub name: String, pub files: Vec<TorrentFile>, pub torrent_bytes: bytes::Bytes }
  pub struct ParsedTorrent { pub info_hash: InfoHash, pub name: Option<String>, pub trackers: Vec<String>, pub meta: Option<TorrentMeta> }
  pub struct StartTorrent { pub info_hash: InfoHash, pub source: TorrentSource, pub save_dir: PathBuf, pub only_files: Option<Vec<usize>>, pub trackers: Vec<String>, pub download_bps: Option<u64> }
  pub struct FileProgress { pub idx: usize, pub done: u64 }
  pub struct TorrentStats { pub selected_total: u64, pub selected_done: u64, pub down_bps: u64, pub up_bps: u64, pub peers_connected: u32, pub peers_seen: u32, pub files: Vec<FileProgress>, pub finished: bool, pub checking: bool, pub has_meta: bool, pub error: Option<String> }
  pub struct PieceMap { pub piece_count: u32, pub have: Vec<u8> /* bitfield, MSB first */ }
  pub struct TorrentSettings { pub listen_port: u16, pub upnp: bool, pub upload_bps: Option<u64> /* Some(0)=no upload, None=unlimited */, pub download_bps: Option<u64> /* ODM's global limit; None=unlimited */, pub peer_limit: usize, pub socks_proxy: Option<String>, pub state_dir: PathBuf, pub extra_peers: Vec<SocketAddr> }
  #[derive(thiserror::Error, Debug)] pub enum TorrentError { Invalid(String), Session(String), NotFound, Io(String), Cancelled }
  #[async_trait] pub trait TorrentClient: Send + Sync { /* exactly the spec's methods */ }
  pub fn parse(source: &TorrentSource) -> Result<ParsedTorrent, TorrentError>;   // in parse.rs, also used by the trait's parse()
  pub fn magnet_info_hash(uri: &str) -> Option<InfoHash>;
  ```
- `TorrentClient::parse` default-impl calls `parse::parse`. `fetch_metadata` returns `TorrentMeta`; `start(StartTorrent)`; `set_files(&InfoHash, &[usize])`; `pause`, `remove(&InfoHash, bool)`; `stats(&InfoHash) -> Option<TorrentStats>`; `pieces(&InfoHash) -> Option<PieceMap>`; `apply_settings(TorrentSettings)`.

- [ ] **Step 1: Write failing tests in `parse.rs`**
  - `magnet_hex_hash`: `magnet:?xt=urn:btih:ABCDEF0123456789ABCDEF0123456789ABCDEF01&dn=Foo&tr=udp%3A%2F%2Ft.example%3A80` → info hash `abcdef0123456789abcdef0123456789abcdef01`, name `Some("Foo")`, trackers `["udp://t.example:80"]`, `meta == None`.
  - `magnet_base32_hash`: 32-char base32 btih decodes to the same hex as its hex form.
  - `magnet_without_btih_is_invalid` → `TorrentError::Invalid`.
  - `not_a_magnet_is_invalid` for `http://x/y`.
  - `torrent_bytes_lists_files`: build a two-file `.torrent` in the test with librqbit's bencode (or a hand-written bencode byte string) → meta with 2 files, `/`-joined paths, sizes, and info hash = SHA-1 of the info dict.
  - `garbage_bytes_are_invalid`.
- [ ] **Step 2:** `cargo test -p odm-torrent` → FAIL (not implemented).
- [ ] **Step 3: Implement** `parse` with librqbit's `librqbit::torrent_from_bytes` / `librqbit_core::magnet::Magnet::parse`; padding files (BEP 47 attr `p`) are excluded but keep their indices out of the list (gaps allowed).
- [ ] **Step 4:** `cargo test -p odm-torrent` → PASS; `cargo clippy -p odm-torrent -- -D warnings` clean.
- [ ] **Step 5: Commit** `feat(torrent): odm-torrent crate with types, trait and parsing`.

### Task 2: Free-port picker

**Files:** Create `crates/odm-torrent/src/port.rs`

**Interfaces:** Produces `pub fn pick_port(preferred: u16) -> Option<u16>` (returns `preferred` if both TCP and UDP bind on `0.0.0.0`, else tries up to 8 random ports in 49152–65535) and `pub fn random_port() -> u16`.

- [ ] **Step 1: Tests:** `free_preferred_is_kept` (bind nothing, ask for a port from `random_port()` that the test just verified free → same port); `busy_tcp_moves_on` (hold a `TcpListener` on port P → result ≠ P, in range); `busy_udp_moves_on` (hold a `UdpSocket` on P → result ≠ P); `random_in_range` (1000 calls all in 49152..=65535).
- [ ] **Step 2:** run → FAIL. **Step 3:** implement with `std::net::{TcpListener, UdpSocket}` and `fastrand`. **Step 4:** run → PASS.
- [ ] **Step 5: Commit** `feat(torrent): TCP+UDP free port picker`.

### Task 3: `LibrqbitClient`

**Files:** Create `crates/odm-torrent/src/rqbit.rs`, `crates/odm-torrent/tests/rqbit.rs`

**Interfaces:**
- Consumes: Task 1 types, Task 2 `pick_port`.
- Produces: `pub struct LibrqbitClient; impl LibrqbitClient { pub fn new(settings: TorrentSettings) -> Arc<Self> }` implementing `TorrentClient`; `pub fn active_port(&self) -> Option<u16>`.

Behaviour to pin:
- Session created lazily on first call that needs it (`OnceCell<Arc<Session>>` guarded by a `tokio::sync::Mutex` for restart), `Session::new_with_opts(default_output = settings.state_dir.join("default"))` with: `persistence` JSON in `state_dir`, DHT persistence in `state_dir`, `listen` with `pick_port(settings.listen_port)` and `enable_upnp_port_forwarding = settings.upnp`, `ratelimits` from settings, `peer_limit`, `connect.proxy_url = socks_proxy`, and **all restored torrents paused** (persisted torrents must not start until the engine calls `start`).
- `fetch_metadata`: `.torrent` → parse locally; magnet → `add_torrent(.., AddTorrentOptions { list_only: true, .. })`, raced against `cancel` (on cancel return `Cancelled`; a list-only add creates no managed handle).
- `start`: `add_torrent` with `paused: false`, `only_files`, `output_folder = save_dir`, `overwrite: true` (resume onto existing data), trackers, per-torrent `ratelimits.download_bps`; `AlreadyManaged` → `update_only_files` then `unpause`.
- `stats`: from `api_stats_v1` + `file_progress`; `selected_total/done` sum over selected files only; `peers_connected`/`peers_seen` from `live.snapshot.peer_stats`; `checking` when state is `initializing`; `has_meta` true once the handle has metadata.
- `remove`: `session.delete(hash, delete_files)`; NotFound is Ok.
- `apply_settings`: rate limits and peer limit live; port/UPnP/proxy change → recreate session only when no torrent is live, otherwise store and apply on next session creation.
- File names Windows cannot create: check in `reserved_names_download`; if librqbit fails or writes a different name, map with `odm_core::filename::sanitize` per path component via librqbit's output-path hook if one exists, otherwise fail that test and stop to ask the user (do not invent a workaround).

- [ ] **Step 1: Write integration tests** in `tests/rqbit.rs`. Shared helper `seed(files: &[(&str, usize)]) -> (Seeder, TorrentSource /*magnet*/, Bytes /*torrent*/, SocketAddr)` creating files in a tempdir and serving them with a second librqbit session via `create_and_serve_torrent`; the client under test reaches it through `TorrentSettings.extra_peers: Vec<SocketAddr>` (empty in production), which `LibrqbitClient` passes as `initial_peers` to both list-only fetches and starts. Tests:
  - `magnet_fetches_metadata` → file list matches seeded names and sizes.
  - `fetch_cancel_returns_cancelled` (no seeder, cancel after 200 ms → `Cancelled` within 2 s).
  - `downloads_selected_files_only` (3 files, select idx 0 and 2; on `finished`, files 0 and 2 byte-identical, `selected_total` = their sizes).
  - `set_files_mid_download` (start with [0], then `set_files(&[0,1])` → both complete).
  - `pause_stops_transfer` (bytes unchanged over 2 s after pause).
  - `restart_resumes_without_redownload` (download half with rate limit, drop client, new client same `state_dir` → stats before `start` show not running; after `start` `selected_done` starts ≥ previous value and finishes).
  - `remove_with_and_without_files`.
  - `reserved_names_download` (seed files `CON/aux.txt` and `a_b.txt`; download succeeds; report on-disk names in the test output).
  - `pieces_map_fills` (after finish, all bits set; `piece_count` > 0).
- [ ] **Step 2:** `cargo test -p odm-torrent --test rqbit` → FAIL.
- [ ] **Step 3: Implement** `LibrqbitClient`.
- [ ] **Step 4:** run → PASS three times in a row (flake check); clippy clean.
- [ ] **Step 5: Commit** `feat(torrent): librqbit client with lazy session and local-seeder tests`.

### Task 4: `FakeTorrentClient`

**Files:** Create `crates/odm-torrent/src/fake.rs` (feature `fake`)

**Interfaces:** Produces
```rust
pub struct FakeTorrentClient;
impl FakeTorrentClient {
  pub fn new() -> Arc<Self>;
  pub fn script(&self, hash: &InfoHash, s: FakeScript);
  pub fn calls(&self) -> Vec<FakeCall>;          // Fetch, Start{only_files}, SetFiles(Vec<usize>), Pause, Remove{delete}, ApplySettings
  pub fn live(&self) -> Vec<InfoHash>;           // hashes currently "in session"
  pub fn advance(&self, hash: &InfoHash, done: u64); // sets selected_done for next stats()
}
pub struct FakeScript { pub meta: Option<TorrentMeta>, pub metadata_after: Option<Duration> /* None = never */, pub fail_start: Option<String> }
```
- [ ] **Step 1: Tests** `fake_records_calls`, `fake_metadata_never_until_cancel`, `fake_finishes_when_done_reaches_total`.
- [ ] **Step 2–4:** fail → implement → pass.
- [ ] **Step 5: Commit** `feat(torrent): scriptable fake client`.

### Task 5: Store V4, `Category::Torrent`, `Completed → Queued`

**Files:** Modify `crates/odm-store/src/{migrations,models,repo,lib}.rs`, `crates/odm-core/src/{category,state}.rs`; tests in their existing test modules.

**Interfaces:** Produces on `Store`:
```rust
pub async fn set_info_hash(&self, id: &str, hash: String) -> Result<(), StoreError>;
pub async fn find_by_info_hash(&self, hash: &str) -> Result<Option<DownloadRecord>, StoreError>;
pub async fn put_torrent_meta(&self, m: TorrentMetaRecord) -> Result<(), StoreError>;
pub async fn get_torrent_meta(&self, id: &str) -> Result<Option<TorrentMetaRecord>, StoreError>;
pub async fn merge_trackers(&self, id: &str, add: Vec<String>) -> Result<Vec<String>, StoreError>; // returns merged, order kept, no dupes
pub async fn replace_torrent_files(&self, id: &str, files: Vec<TorrentFileRecord>) -> Result<(), StoreError>; // also sets has_meta=1, downloads.size = selected total
pub async fn get_torrent_files(&self, id: &str) -> Result<Vec<TorrentFileRecord>, StoreError>;
pub async fn set_torrent_selection(&self, id: &str, selected: Vec<usize>) -> Result<u64, StoreError>; // returns new selected total, written to downloads.size
pub struct TorrentMetaRecord { pub download_id: String, pub source: String, pub trackers: Vec<String>, pub has_meta: bool }
pub struct TorrentFileRecord { pub idx: usize, pub path: String, pub size: u64, pub selected: bool }
```
`DownloadRecord` gains `info_hash: Option<String>` and `DownloadRecord::new_torrent(id, info_hash, name, save_dir)` (`kind="torrent"`, `category="torrent"`, `max_connections` unused = 0). `Category::Torrent` (`"torrent"`, default folder `"Torrents"`, not matched by `by_ext`). `can_transition(Completed, Queued)` = true.

- [ ] **Step 1: Tests:** `v4_from_fresh` and `v4_from_v3` (user_version 4, columns/tables exist); `info_hash_unique` (second insert with same hash fails); `merge_trackers_dedupes`; `selection_updates_size`; `cascade_deletes_torrent_rows`; `torrent_category_round_trips` and `torrent_not_by_ext`; `completed_can_requeue`.
- [ ] **Step 2–4:** fail → implement (append `V4` to `MIGRATIONS`, SQL exactly as in the spec) → `cargo test -p odm-store -p odm-core` pass.
- [ ] **Step 5: Commit** `feat(store): torrent tables (V4), torrent category, completed->queued`.

### Task 6: Engine — adding torrents and metadata

**Files:** Create `crates/odm-engine/src/torrent/mod.rs`; modify `manager.rs`, `config.rs`, `events.rs`, `lib.rs`, `Cargo.toml` (`odm-torrent`, dev-dep with `fake`); tests `crates/odm-engine/tests/torrent.rs`.

**Interfaces:**
- Consumes: Tasks 1, 4, 5.
- Produces:
  ```rust
  pub struct TorrentRequest { pub source: TorrentSource, pub save_dir: Option<PathBuf>, pub stored_torrent: Option<PathBuf> /* where appcore saved .torrent bytes */ }
  pub struct TorrentAdded { pub id: String, pub existing: bool }
  impl ManagerHandle {
    pub async fn start_with_torrents(config, store, clock, torrents: Arc<dyn TorrentClient>) -> Result<ManagerHandle, OdmError>; // `start` uses LibrqbitClient::new(config.torrent.clone())
    pub async fn add_torrent(&self, req: TorrentRequest) -> Result<TorrentAdded, OdmError>;
    pub async fn torrent_files(&self, id: &str) -> Result<Vec<TorrentFileRecord>, OdmError>;
  }
  EngineEvent::TorrentMetadata { id: String }   // json {"type":"torrent_metadata","id":..}
  EngineConfig.torrent: TorrentSettings          // Default: port 0 (= choose), upnp true, upload Some(102_400), peer_limit 100
  ```
- Behaviour: `add_torrent` parses; dedupe via `find_by_info_hash` → `merge_trackers`, return `existing: true`; else insert `new_torrent` row as `Paused` (no `pause_reason`), `torrent_meta` (`source` = magnet URI or stored path), files if `.torrent`; then spawn a metadata fetcher (`CancellationToken` kept in a `HashMap<String, CancellationToken>` in the actor) for magnets. On metadata: `replace_torrent_files` (all selected), filename = meta name when the row has the magnet `dn` or none, emit `TorrentMetadata`. Cancel/remove of a fetching row cancels the token. Invalid source → `ErrorKind::Other` with message "This isn't a valid magnet link or torrent file." and no row. `set_speed_limit` / `set_config` also call `torrents.apply_settings` with `download_bps` = the engine's global `speed_limit_bps` (0 → None); torrents use their own librqbit bucket at the same cap rather than sharing ODM's limiter.
- Peer count while a magnet waits for its file list: librqbit's list-only add exposes peers only when it returns, so the waiting row shows "Fetching file list…" without a count; the count appears once the torrent is in the session (Probing / Downloading). This narrows the spec's "· N peers" for the pre-confirm wait; tell the user at review.

- [ ] **Step 1: Tests (fake client):** `speed_limit_reaches_torrents` (`set_speed_limit(500_000)` → `ApplySettings` call with `download_bps: Some(500_000)`); `magnet_adds_paused_row_then_metadata` (event `TorrentMetadata`, files stored, `size` = total); `torrent_bytes_have_files_immediately` (no fetch call); `duplicate_merges_trackers`; `invalid_source_creates_no_row`; `remove_during_metadata_drops_handle` (script `metadata_after: None`; remove → fetch cancelled, row gone, `live()` empty); `event_json_shape_is_pinned` extended for `torrent_metadata`.
- [ ] **Step 2–4:** fail → implement → `cargo test -p odm-engine` pass.
- [ ] **Step 5: Commit** `feat(engine): add torrents with dedupe and metadata fetch`.

### Task 7: Engine — `run_torrent`, file changes, finish

**Files:** Create `crates/odm-engine/src/torrent/{run,progress}.rs`; modify `manager.rs` (`launch` dispatch, `work_dir`, `remove_partials`, cancel/remove), `events.rs`; tests in `crates/odm-engine/tests/torrent.rs`.

**Interfaces:**
- Produces:
  ```rust
  pub async fn run_torrent(env: &DownloadEnv, torrents: &Arc<dyn TorrentClient>, id: &str, cancel: CancellationToken) -> Outcome;
  pub struct Monotonic { /* last reported */ } impl Monotonic { pub fn report(&mut self, done: u64, total: u64) -> u64 }
  impl ManagerHandle {
    pub async fn set_torrent_files(&self, id: &str, selected: Vec<usize>) -> Result<(), OdmError>;
    pub async fn pieces(&self, id: &str) -> Result<Option<PieceMap>, OdmError>;
  }
  ProgressSnapshot.torrent: Option<TorrentProgress>  // #[serde(skip_serializing_if = "Option::is_none")]
  pub struct TorrentProgress { pub peers_connected: u32, pub peers_seen: u32, pub up_bps: u64 }
  ```
- `run_torrent`: `Queued→Probing`; `start` with selected indices (None if no metadata yet), trackers from `torrent_meta`, row `speed_limit`; poll `stats` every `config.progress_interval`; `Probing→Downloading` once `has_meta && !checking`; when metadata appears during the run and the table is empty, write files (all selected) and emit `TorrentMetadata`; emit `Progress` with `Monotonic::report`; persist bytes every `persist_interval`; `stats.error` → `Outcome::Failed` (`ErrorKind::Io` for disk errors); `finished` → `pause` then `remove(false)` then `Outcome::Completed { path: save_dir.join(name) }` without the `Exists` flow (torrents write in place). Cancel token → `pause`, `Outcome::Stopped`.
- `Monotonic::report`: never returns less than its previous value, except it re-clamps to `total` when `total` shrinks.
- `set_torrent_files`: empty → error "Keep at least one file selected."; store selection; if live → `set_files`; if `Completed` and the selection adds a file that is not complete → state `Queued` and kick the queue.
- Cancel/remove of a torrent row → `remove(hash, delete_file)` and delete `stored_torrent`.

- [ ] **Step 1: Tests (fake client):** `runs_to_completed_and_leaves_session` (`calls()` ends with `Pause, Remove{delete:false}`; `live()` empty); `progress_never_decreases` (advance 50→40→60 → reported 50,50,60); `shrink_reclamps` (unit test on `Monotonic`); `set_files_live_calls_client`; `set_files_rejects_empty`; `retick_completed_requeues`; `start_failure_fails_row` (`fail_start`) and `http_downloads_unaffected_by_torrent_failure` (an HTTP download via odm-testserver completes alongside); `pause_then_resume_restarts_client`; `cancel_deletes_files` (`Remove{delete:true}`); progress JSON includes `"torrent":{...}` only for torrents.
- [ ] **Step 2–4:** fail → implement → `cargo test -p odm-engine` pass; clippy clean.
- [ ] **Step 5: Commit** `feat(engine): run torrents, live file selection, stop on completion`.

### Task 8: Appcore — source resolution, API, IPC, settings, DTOs

**Files:** Create `crates/odm-appcore/src/torrent.rs`; modify `api.rs`, `ipc.rs`, `dto.rs`, `settings.rs`, `paths.rs`, `odm-core/src/ipc.rs`; tests in `crates/odm-appcore/tests/{api,ipc,settings}.rs`.

**Interfaces:**
- Produces:
  ```rust
  // odm-core::ipc
  pub const ADD_TORRENT: &str = "ADD_TORRENT";
  pub struct AddTorrent { pub link: String, pub kind: TorrentLinkKind /* "magnet"|"url" */, pub referrer: Option<String>, pub headers: BTreeMap<String,String>, pub cookies: Option<String>, pub source: Source }
  // appcore::torrent
  pub async fn resolve(input: TorrentInput, http: &reqwest::Client, torrents_dir: &Path) -> Result<TorrentRequest, String>;
  pub enum TorrentInput { Text(String) /* magnet */, File(PathBuf), Url { url: String, headers: Vec<(String,String)> } }
  pub const MAX_TORRENT_BYTES: usize = 10 * 1024 * 1024; pub const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
  // appcore::api
  pub async fn add_torrent(m: &ManagerHandle, pending: &Pending, input: TorrentInput, ...) -> Result<TorrentAdded, String>; // always pending.push unless existing
  ConfirmArgs.files: Option<Vec<usize>>  // torrents: Some = selection; None with action "start" = all
  DownloadDto { info_hash: Option<String>, has_meta: Option<bool>, ... }
  DetailDto.torrent: Option<TorrentDetailDto>
  pub struct TorrentDetailDto { pub info_hash: String, pub files: Vec<TorrentFileDto>, pub pieces: Option<PieceMapDto>, pub trackers: usize }
  pub struct TorrentFileDto { pub idx: usize, pub path: String, pub size: u64, pub done: u64, pub selected: bool }
  pub struct PieceMapDto { pub count: u32, pub have_b64: String }
  Settings { torrent_port: u16 /* 0 until first run picks one */, torrent_upnp: bool, torrent_upload_kbps: Option<u64>, torrent_max_peers: u32 }
  ```
- `resolve`: `.torrent` bytes are written to `torrents_dir/files/<hash>.torrent`; URL fetch refuses > 10 MB (by `Content-Length` and while streaming) with "The torrent file is larger than 10 MB." and times out at 15 s with "The torrent file didn't download in time."
- First settings load with `torrent_port == 0` stores `odm_torrent::port::random_port()`.
- `to_engine_config` maps the four fields plus socks proxy (only `socks5://`/`socks5h://` from manual proxy) and `state_dir = paths::data_dir().join("torrents")`.

- [ ] **Step 1: Tests:** `ipc_add_torrent_magnet` (pending contains id even with confirm setting off); `ipc_add_torrent_url_fetches` (odm-testserver serves a `.torrent`); `url_fetch_too_large` (testserver serves 11 MB); `url_fetch_timeout` (testserver delays > 15 s — use a configurable timeout parameter set to 200 ms in the test); `confirm_torrent_with_files` (selection stored, resumed); `confirm_download_all_without_meta`; `settings_torrent_defaults_and_parse` (port chosen in range on first load, kept after); `socks_only_proxy_reaches_torrent`; `detail_has_torrent_section`.
- [ ] **Step 2–4:** fail → implement → `cargo test -p odm-appcore -p odm-core` pass.
- [ ] **Step 5: Commit** `feat(appcore): torrent intake, confirm with files, settings and detail`.

### Task 9: Tauri commands and command-line intake

**Files:** Modify `app/src-tauri/src/{commands,lib}.rs`, `app/src/lib/{api,types}.ts`; test `app/src-tauri/tests/` (existing pattern) for argv parsing.

**Interfaces:**
- Produces Tauri commands `add_torrent(input: TorrentInputDto) -> TorrentAddedDto`, `torrent_files(id) -> Vec<TorrentFileDto>`, `set_torrent_files(id, selected: Vec<usize>)`, `pick_torrent_file() -> Option<String>` (dialog filter `*.torrent`); UI event `TorrentMetadata { id }` and `Focus { id }` (for `existing: true`) on `odm://event`.
- `pub fn torrent_arg(argv: &[String]) -> Option<TorrentInput>`: first arg starting with `magnet:` (case-insensitive) or ending `.torrent` that exists as a file. Called at startup (after the window is ready) and in the single-instance callback.

- [ ] **Step 1: Tests:** `torrent_arg_magnet`, `torrent_arg_file`, `torrent_arg_ignores_background_flag`, `torrent_arg_missing_file_is_none`.
- [ ] **Step 2–4:** fail → implement → `cargo test -p odm-app` pass; `pnpm tauri build --debug --no-bundle` compiles.
- [ ] **Step 5: Commit** `feat(app): torrent commands and magnet/.torrent arguments`.

### Task 10: UI — add, confirm file tree, detail, settings

**Files:** Create `app/src/lib/fileTree.ts`, `app/src/lib/fileTree.test.ts`, `app/src/components/TorrentFiles.svelte`; modify `AddDialog.svelte`, `ConfirmDialog.svelte`, `DetailPanel.svelte`, `ChunkMap.svelte`, `SettingsPage.svelte`, `lib/model.ts`, `lib/downloads.svelte.ts`.

**Interfaces:**
- Produces in `fileTree.ts`:
  ```ts
  export type FileNode = { name: string; path: string; size: number; idx?: number; children: FileNode[] };
  export function buildTree(files: { idx: number; path: string; size: number }[]): FileNode[];
  export function toggle(sel: Set<number>, node: FileNode, on: boolean): Set<number>;
  export function nodeState(sel: Set<number>, node: FileNode): "on" | "off" | "mixed";
  export function selectedSize(files: { idx: number; size: number }[], sel: Set<number>): number;
  ```
- `TorrentFiles.svelte` renders the tree with tick boxes (props: files, selected, onchange, progress?), used by both dialogs.
- AddDialog: a magnet in the URL box routes to `add_torrent`; "Torrent file…" button uses `pick_torrent_file`; dropping a `.torrent` file on the window opens it.
- ConfirmDialog torrent mode: shows "Fetching file list…" until `has_meta`, then the tree, selected total, free space; **Download** disabled with no metadata or empty selection; **Download all** always enabled; refetches files on `TorrentMetadata`.
- DetailPanel torrent mode: header line `info hash · N peers (M seen) · ↑ speed`; `TorrentFiles` with progress and live ticks calling `set_torrent_files` (show the "Keep at least one file selected." error inline); ChunkMap draws `PieceMapDto` (decode base64 bitfield; group pieces into the map's existing cell count).
- Row status labels for torrents per the spec's States table.
- Settings "Torrents" card: listen port (NumberField + "Random" button), UPnP switch, upload limit KB/s (empty = unlimited, 0 = no upload), max peers; the proxy note when proxy mode is manual and not SOCKS5.

- [ ] **Step 1: Tests (vitest)** in `fileTree.test.ts`: `builds_nested_folders`, `folder_toggle_selects_children`, `mixed_state`, `selected_size_sums`; in `model.test.ts`: `torrent_status_labels` (fetching / waiting / checking / peers).
- [ ] **Step 2–4:** fail → implement → `cd app && pnpm test && pnpm check` pass.
- [ ] **Step 5: Manual look** in a debug build (launched from PowerShell): add a public magnet, see fetching → tree → download → detail panel files, peers, piece map; screenshot for the user.
- [ ] **Step 6: Commit** `feat(ui): torrent add, file chooser, detail files and piece map, settings card`.

### Task 11: Extension — magnet links and `.torrent` downloads

**Files:** Create `extension/src/lib/torrent.ts`, `extension/src/lib/torrent.test.ts`; modify `extension/src/background.ts`, `extension/src/content.ts`, manifests if a permission is needed (none expected).

**Interfaces:**
- Produces:
  ```ts
  export function isMagnet(url: string): boolean;                 // case-insensitive "magnet:?"
  export function isTorrentDownload(item: { filename?: string; finalUrl?: string; url: string; mime?: string }): boolean; // .torrent ext or application/x-bittorrent
  export function toTorrentPayload(link: string, kind: "magnet" | "url", referrer: string | undefined, cookies: string | undefined, ua: string): AddTorrentPayload;
  ```
- content.ts: capture-phase click listener on `a[href^="magnet:" i]` (not with Alt held, matching the existing Alt bypass) → `preventDefault` and send a runtime message `{ type: "odm-magnet", url }`; background sends `ADD_TORRENT`; on no answer, fall back by navigating the tab to the magnet so the OS handler still works.
- Context menu click with a magnet `linkUrl` sends `ADD_TORRENT { kind: "magnet" }` (today it is dropped by `isHttp`).
- `handle()`: `isTorrentDownload` items go as `ADD_TORRENT { kind: "url" }` with cookies; same cancel/erase logic as `ADD_DOWNLOAD`.

- [ ] **Step 1: Tests:** `is_magnet_variants`, `torrent_by_extension_and_mime`, `payload_shape` (pins JSON keys `link, kind, referrer, headers, cookies, source`), and in `wiring.test.ts` that the click handler is registered.
- [ ] **Step 2–4:** fail → implement → `cd extension && pnpm test && pnpm check && pnpm build` pass.
- [ ] **Step 5: Commit** `feat(extension): send magnet links and .torrent downloads to ODM`.

### Task 12: Installer associations

**Files:** Modify `app/src-tauri/installer-hooks.nsh` (and `tauri.conf.json` NSIS section if a components page or custom page is needed).

Behaviour: two optional, unticked-by-default choices "Open magnet links with ODM" and "Open .torrent files with ODM". Install writes per-user keys: `HKCU\Software\Classes\magnet` (`URL Protocol`, `shell\open\command` = `"$INSTDIR\ODM.exe" "%1"`) and `HKCU\Software\Classes\.torrent` → `ODM.torrent` ProgID with the same command. Uninstall (not `/UPDATE`) removes them only if the command still points to `$INSTDIR\ODM.exe`. Updates keep the user's earlier choice (do not rewrite or delete during `/UPDATE`).

- [ ] **Step 1:** implement; build with `node scripts/release.mjs` (or the debug NSIS build used in Plan 5) → exit 0.
- [ ] **Step 2:** add the install/uninstall/update checks to the smoke checklist (Task 13) for the user to run from their own session.
- [ ] **Step 3: Commit** `feat(installer): optional magnet and .torrent associations`.

### Task 13: Smoke checklist and full verification

**Files:** Create `docs/superpowers/plans/2026-10-23-odm-torrents-smoke.md`; update `CHANGELOG.md` (Unreleased: "Torrent downloads").

- [ ] **Step 1:** write the checklist: public magnet (e.g. a Linux ISO) end to end; untick a file mid-download; pause, quit, reopen, resume; finish → no upload afterwards (Task Manager network or the row's ↑ 0); magnet click in Chrome and Firefox; `.torrent` download from a site; dead magnet stays "Fetching file list…" and removes cleanly; installer associations: install with both ticked, double-click `.torrent`, click a magnet in a browser with ODM closed and open, uninstall removes keys, update keeps them; firewall prompt appears once on first listen.
- [ ] **Step 2:** run all Global Constraints checks; expected all PASS.
- [ ] **Step 3: Commit** `docs: torrent smoke checklist and changelog`; ask the user whether to push.
