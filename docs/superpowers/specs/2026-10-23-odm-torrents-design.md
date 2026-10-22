# Torrent downloads — design

**Status:** agreed in conversation on 2026-10-23; awaiting written-spec review.
**Branch:** `master` (ODM keeps a single branch).

## Goal

Let ODM download magnet links and `.torrent` files as ordinary downloads: same list, same queue, same pause / resume / cancel / remove, persistent across restarts. The user can **follow a magnet download closely** (file-list fetch, peers, speed, per-file progress, piece map) and **control which files are downloaded**, both when adding and while it runs.

ODM is a downloader, not a seeding client: a torrent is finished when its selected files are complete, and ODM then stops it.

### Decisions

| Topic | Decision |
|---|---|
| Scope | Download-only. No seeding management (no ratio / seed-time limits, no seeding state, no seed queue). |
| Engine | `librqbit` 9.0.1, in process, wrapped by a new crate `odm-torrent`. Rejected: librqbit directly inside `odm-engine` (its types leak into the manager, every engine test build compiles it); a sidecar `rqbit`/aria2c exe (second binary, port, supervision). |
| Intake | Add dialog (paste magnet, browse / drop `.torrent`), browser extension (magnet clicks and `.torrent` downloads), optional Windows `magnet:` / `.torrent` associations. |
| File choice | Always asked in the confirm dialog. A magnet shows "Fetching file list…" until metadata arrives; **Download all** is available without waiting. Files can be changed later in the detail panel. |
| Piece map | Yes: the detail panel's chunk map shows the torrent's pieces. |
| Upload | Allowed while downloading, capped (default 100 KB/s) because peers reward uploaders; 0 means no upload. Stops when the torrent completes. |

### Out of scope

Seeding and seeding limits, RSS, tracker list editing, peer list / banning, sequential download, protocol encryption (librqbit has none), torrent creation, a firewall rule in the installer (Windows asks on first listen), GeoIP.

### Background

A full libtorrent-based client was built in the Python prototype (`Personal Download Manager`, branch `feature/torrent-client`). That version is kept as is; it is not touched by this work. Its ledger contributed the lessons marked *(lesson)* below.

## Architecture

### New crate `crates/odm-torrent`

The only crate that depends on librqbit. It exposes a trait and one real implementation:

```rust
#[async_trait]
pub trait TorrentClient: Send + Sync {
    /// Parses a magnet link or .torrent bytes. No network.
    fn parse(&self, source: &TorrentSource) -> Result<ParsedTorrent, TorrentError>; // info hash, name, trackers, files if known
    /// Fetches the file list (list-only add for magnets; immediate for .torrent). Cancellable.
    async fn fetch_metadata(&self, source: &TorrentSource, cancel: CancellationToken) -> Result<TorrentMeta, TorrentError>;
    /// Starts (or resumes from fastresume) downloading the selected files into `save_dir`.
    async fn start(&self, req: StartTorrent) -> Result<(), TorrentError>;
    async fn set_files(&self, info_hash: &InfoHash, selected: &[usize]) -> Result<(), TorrentError>;
    async fn pause(&self, info_hash: &InfoHash) -> Result<(), TorrentError>;
    /// Removes from the session; `delete_files` also deletes downloaded data.
    async fn remove(&self, info_hash: &InfoHash, delete_files: bool) -> Result<(), TorrentError>;
    fn stats(&self, info_hash: &InfoHash) -> Option<TorrentStats>;
    fn pieces(&self, info_hash: &InfoHash) -> Option<PieceMap>;
    async fn apply_settings(&self, s: TorrentSettings) -> Result<(), TorrentError>;
}
```

- `LibrqbitClient` owns one `librqbit::Session`, created lazily on first use, with persistence (JSON + fastresume) under `%APPDATA%\ODM\torrents\` and DHT state alongside.
- `FakeTorrentClient` (behind a `fake` feature) is scriptable: metadata delay, progress steps, failures. Engine and appcore tests use it.
- `TorrentStats`: `selected_total`, `selected_done` (monotonic, see Progress), `down_bps`, `up_bps`, `peers_connected`, `peers_seen`, per-file `done` bytes, `finished` (all selected files complete).
- `TorrentMeta`: name, total size, files `(index, path, size)` with padding files excluded and **indices kept as librqbit gives them (they may have gaps)** *(lesson)*.

### `odm-engine`

- New kind `"torrent"`. `launch()` dispatches it to `run_torrent`, next to `run_hls` and `run_site`. `work_dir` / `remove_partials` learn the kind (partials = librqbit delete with files).
- `EngineConfig` gains `torrent: TorrentSettings` and the manager holds an `Arc<dyn TorrentClient>`.
- New manager methods: `add_torrent(TorrentRequest)`, `torrent_files(id)`, `set_torrent_files(id, selected)`, `pieces(id)`.
- Metadata fetches for rows waiting on confirmation run **outside the download queue** (they are small) in a `MetadataFetcher` task per row, cancelled when the row is removed or confirmed with "Download all" before metadata arrives (the full start then fetches it).

### `odm-store`

Migration V4 (append only):

```sql
ALTER TABLE downloads ADD COLUMN info_hash TEXT;          -- lowercase hex, torrents only
CREATE UNIQUE INDEX idx_downloads_info_hash ON downloads(info_hash) WHERE info_hash IS NOT NULL;
CREATE TABLE torrent_meta (
    download_id TEXT PRIMARY KEY REFERENCES downloads(id) ON DELETE CASCADE,
    source      TEXT NOT NULL,    -- magnet URI, or path of ODM's stored .torrent copy
    trackers    TEXT NOT NULL DEFAULT '[]',  -- JSON array, merged on duplicate adds
    has_meta    INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE torrent_files (
    download_id TEXT NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,  -- librqbit file index
    path        TEXT NOT NULL,     -- original path inside the torrent, for display
    size        INTEGER NOT NULL,
    selected    INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (download_id, idx)
);
```

`.torrent` sources are copied to `%APPDATA%\ODM\torrents\files\<info_hash>.torrent` so the row survives the original being moved. `downloads.size` holds the selected total; `downloads.filename` holds the torrent name (single-file torrent: the file name).

### `odm-appcore` and the app

- `api::add_torrent(source)`, used by the Add dialog, the IPC `AddTorrent` message and the command line.
- `odm-core::ipc` gains `AddTorrent { link: String, kind: "magnet" | "url", referrer, headers, cookies, source: Source }` (`link` is the magnet URI or the `.torrent` URL; `source` is the existing capture/menu enum).
- New Tauri commands: `add_torrent`, `torrent_files`, `set_torrent_files`; `get_detail` adds `torrent: Option<TorrentDetailDto>` (info hash, peers, up speed, files with progress, piece map).
- UI: Add dialog accepts magnets and `.torrent` (browse + drop); ConfirmDialog gets a file-tree mode; DetailPanel gets a Files list and a torrent header line; ChunkMap renders the piece map.

## Lifecycle

### States

Torrents reuse `DownloadState`; no new states.

| Situation | State | UI label |
|---|---|---|
| Waiting for the confirm dialog | `Paused` (pending confirm, as for browser downloads) | "Fetching file list…" until metadata, then "Waiting for you" |
| Queued | `Queued` | Queued |
| In the session, metadata not yet known (Download all before metadata) | `Probing` | Fetching file list… · N peers |
| Checking existing data on start / resume | `Probing` | Checking files… |
| Transferring | `Downloading` | speed · peers |
| Selected files complete | `Completed` | Completed |

### Adding

1. `add_torrent(source)`: parse. A bad magnet / `.torrent` is rejected here and no row is created.
2. **Dedupe by info hash** *(lesson)*. If a row exists: merge new trackers into `torrent_meta.trackers`, emit an event that selects that row, and stop. Merged trackers are passed to librqbit the next time the torrent starts.
3. Create the row (`kind="torrent"`, `category="torrent"`, default folder = the existing category folder logic) as `Paused`, pending confirm, and start the `MetadataFetcher`. A `.torrent` has metadata immediately.
4. When metadata arrives: write `torrent_files` (all selected), set `has_meta`, emit `TorrentMetadata { id }`. The UI refetches the detail and the dialog fills its file tree.

### Confirm dialog

File tree with tick boxes, sizes and folders (folders tick their children), select all / none, selected total and free space on the chosen drive, save folder. Buttons: **Download** (needs metadata and at least one file), **Download all** (always), **Cancel**. Cancel removes the row and cancels the fetch, which removes any list-only handle *(lesson)*.

### Downloading

`run_torrent` takes a queue slot, calls `start` with the selected indices, trackers, per-torrent speed limit and save folder, then polls `stats` once a second:

- emits the existing `Progress` snapshot with `selected_done` / `selected_total` / `down_bps` and ETA;
- adds `peers_connected`, `peers_seen`, `up_bps` to the snapshot (optional fields, so other kinds are unchanged);
- **progress never goes backwards** in what ODM reports: a lower `selected_done` than last reported is held until real progress passes it, and re-clamped when the selection shrinks *(lesson)*;
- updates per-file `done` for the detail panel (store write at most every 5 s).

### Changing files while it runs

`set_torrent_files(id, selected)` updates `torrent_files.selected` and `downloads.size`, then calls `set_files` (librqbit `update_only_files`) when the torrent is in the session. At least one file must stay selected. Unticking keeps data already on disk; pieces shared with a selected neighbour may still write a few bytes of an unticked file.

### Pause, resume, restart

Pause → `pause` (librqbit keeps fastresume); resume → back to `Queued`, then `start` resumes without a full recheck. On app start, active rows return to `Queued` as today; librqbit's persistence restores its torrents paused, and ODM starts them only via the queue.

### Finishing — no seeding

When `stats.finished`: `pause`, then `remove(delete_files = false)`, then `Completed`, `finished_at` set, completion notification and after-queue actions as for other downloads. If the user later ticks more files on a completed torrent, it returns to `Queued` and is re-added; librqbit verifies the existing data and downloads only the new files.

### Cancel and remove

`cancel(id, delete_file)` / `remove(id, delete_file)` call `remove(info_hash, delete_files)` (or cancel the `MetadataFetcher`), delete the stored `.torrent` copy, then follow the existing flow.

## Settings

New **Torrents** card on the settings page:

| Setting | Default | Notes |
|---|---|---|
| Listen port | random in 49152–65535, chosen on first run | Checked free for **TCP and UDP**; on failure retry a few random ports, because Windows excluded UDP ranges can leave a port unusable *(lesson)*. |
| UPnP port forwarding | on | `ListenerOptions.enable_upnp_port_forwarding` |
| Upload limit while downloading | 100 KB/s | 0 = no upload; empty = unlimited |
| Max peers per torrent | 100 | `peer_limit` |

Download speed follows ODM's global limit and per-download limits like any other download. **Proxy:** a manual `socks5://` / `socks5h://` proxy is passed to librqbit (`connect.proxy_url`). An `http(s)://` proxy cannot carry peer traffic, so torrents connect directly and the card says so. Changing settings calls `apply_settings`; port changes restart the session at the next idle moment (no torrent in the session) or app start.

## Errors

| Case | Behaviour |
|---|---|
| Invalid magnet / `.torrent` | Rejected in the Add dialog or IPC reply; no row. |
| `.torrent` URL from the browser | Fetched by the app with the captured headers and cookies, max 10 MB, 15 s timeout, off the UI thread *(lesson)*; failure fails the add with the reason. |
| No peers / no metadata yet | Not an error. The row shows the peer count; the user can wait, pause or remove. |
| Disk full / write error | `Failed` with the reason; Resume restarts with a recheck. |
| Session cannot start (port, persistence) | Torrent rows fail with a clear message; HTTP, HLS and site downloads are unaffected. |
| Names Windows cannot create (`CON`, `aux.txt`, `:` …) | Sanitized with `odm-core::filename` rules on disk if librqbit does not already do so (verified in the plan's first task); `torrent_files.path` keeps the original for display *(lesson)*. |
| Info hash already present | Dedupe flow above, not an error. |

## Integration

- **Extension** (`extension/src/background.ts`): magnet link clicks and the "Download with ODM" context menu on magnet links send `AddTorrent { kind: "magnet" }`; `.torrent` downloads (by extension or `application/x-bittorrent`) are cancelled in the browser and sent as `AddTorrent { kind: "url" }` with headers and cookies. The confirm setting for browser downloads does not apply: torrents always show the file chooser.
- **Command line:** `ODM.exe <magnet-uri | path.torrent>`. A second launch forwards its arguments through the existing single-instance plugin; the first launch processes the argument after the window is ready.
- **Installer** (`installer-hooks.nsh`): optional components, off by default, to register `magnet:` and `.torrent` for the current user → `"ODM.exe" "%1"`; removed on uninstall only if they still point to ODM.

## Testing

- `odm-torrent` integration tests against a local seeder made with librqbit's `create_and_serve_torrent` on localhost: magnet → metadata, `.torrent` start, selected files only, `set_files` mid-download, pause / resume across a client restart (fastresume), finish → removed from session, delete with files, reserved Windows names.
- `odm-engine` tests with `FakeTorrentClient`: state flow, dedupe and tracker merge, cancel during metadata, monotonic progress (including shrink), finish stops the torrent, re-tick on a completed torrent, session-start failure isolates torrents.
- `odm-store` migration V4 test (fresh and from V3).
- `odm-appcore`: IPC `AddTorrent` (magnet and url), `.torrent` URL fetch limits, settings parse / validation for the Torrents card.
- UI unit tests (vitest): file-tree selection logic (folder ticks, totals, at-least-one rule).
- Extension unit tests for magnet / `.torrent` detection.
- Manual smoke checklist `docs/superpowers/plans/2026-10-23-odm-torrents-smoke.md`: real public torrent by magnet, browser magnet click, `.torrent` download from a site, Windows associations from an installed build (started by the user, not by Claude, because of the MSIX container), restart mid-download.
