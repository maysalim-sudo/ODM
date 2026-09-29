# ODM (Open Downloader Manager): Design Spec

- **Date:** 2026-09-29
- **Status:** Approved in conversation; waiting for written-spec review
- **Replaces:** `E:\Data From Desktop\Coding\Personal Download Manager`, the Python/PySide6 prototype. Its audit is in that folder's `PROJECT_ANALYSIS.md`. The old folder stays untouched as a reference.

---

## 1. Purpose and success criteria

**Purpose:** a personal, daily-use download manager for Windows, like IDM, built and used by one person. Reliability matters more than polish, and there is no public distribution in v1.

**v1 is successful when:**
1. Clicking a download link in Chrome/Chromium or Firefox/Floorp hands it to ODM with its cookies and Referer, and ODM finishes it correctly. This includes downloads that require a login.
2. Downloads use several connections and finish noticeably faster than the browser on servers that support byte ranges.
3. Pausing, killing the app, or rebooting mid-download, then resuming, produces a byte-identical file.
4. HLS videos found on a page can be sent to ODM and come out as a playable `.mp4`.
5. The queue, concurrency limit, speed limit and automatic retries behave as configured.
6. A detail view shows per-connection progress as a live chunk map.
7. Every bug listed in the old audit has a regression test that passes.

**Not in v1:** DASH download (it is detected, and the UI says "not supported yet"), scheduling, Linux/macOS builds, auto-update, public installer and signing, BitTorrent, and a portable mode.

---

## 2. Identity (use exactly these everywhere)

| Item | Value |
|---|---|
| Product name | ODM (Open Downloader Manager) |
| Tauri bundle identifier | `com.odm.app` |
| Native messaging host name | `com.odm.host` |
| Firefox/Floorp extension ID | `odm@odm.local` |
| Chromium extension ID | Fixed by a `key` in the Chromium manifest. The ID derived from that key is stored in `extension/chromium-id.txt` and used by `odm-host --register`. |
| Named pipe | `\\.\pipe\odm-<windows-username>` |
| App data directory | `%APPDATA%\ODM\` (`odm.db`, `settings.json`, `logs\`) |
| Default download directory | `%USERPROFILE%\Downloads` plus category subfolders |

All of these constants live in one crate (`odm-core::ids`) and one TS module (`extension/src/ids.ts`). Nothing else hard-codes them.

---

## 3. Stack

- **Backend:** Rust (stable), `tokio`, `reqwest` (rustls, streaming), `rusqlite` (bundled), `serde`, `thiserror`, `tracing` + `tracing-appender`.
- **Desktop shell:** Tauri 2 (tray, single-instance, notification and dialog plugins).
- **UI:** Svelte 5 + TypeScript + Vite, served by Tauri.
- **Extension:** TypeScript, built with Vite into two outputs (Chromium MV3 and Firefox MV3).
- **FFmpeg:** external. Found through the `ffmpeg_path` setting, then PATH. It is not bundled.
- **Toolchain prerequisites (not yet installed):** rustup (stable MSVC) and the Visual Studio 2022 C++ Build Tools. Node 24, pnpm, git and FFmpeg 9.0 are already installed.

---

## 4. Architecture

```
Browser extension ──native messaging (stdio)──▶ odm-host.exe ──named pipe──▶ ODM app (Tauri)
                                                  (small Rust bin)             ├─ engine  (tokio)
                                                  starts app if needed         ├─ hls
                                                                               ├─ store   (SQLite)
                                                                               ├─ ipc     (pipe server)
                                                                               └─ UI      (Svelte webview)
```

### 4.1 Repository layout

```
ODM/
  Cargo.toml                 (workspace)
  crates/
    odm-core/                shared types: ids, DownloadState, IPC messages, errors (no I/O)
    odm-engine/              manager, per-download task, chunk workers, limiter, retry, HLS
    odm-store/               SQLite schema + migrations + repository (single owner thread)
    odm-host/                native messaging host binary + --register/--unregister
    odm-cli/                 dev tool: `odm-cli get <url>` drives the engine without UI
  app/
    src-tauri/               Tauri app: wires engine + store + ipc + UI commands/events
    src/                     Svelte UI
  extension/
    src/                     shared TS (background, content, popup)
    manifests/chromium.json, manifests/firefox.json
  tests/                     (Rust integration tests live in each crate's tests/)
  docs/superpowers/specs/, docs/superpowers/plans/
```

Each crate has one job and depends only on the crates below it: `odm-core` ← `odm-store` ← `odm-engine` ← `app`. `odm-host` depends only on `odm-core`.

### 4.2 IPC

- **Browser ↔ host:** standard native messaging. Each message is a 4-byte little-endian length followed by UTF-8 JSON, and must be ≤ 1 MB.
- **Host ↔ app:** a named pipe using the same length-prefixed JSON framing. The pipe is created with a security descriptor that only allows the current user.
- **Host behaviour:**
  - It loops over stdin messages until EOF, forwarding each one and writing the reply to stdout.
  - If the pipe is missing, it starts `ODM.exe --background` (path stored at registration) and retries for up to 10 s.
  - It **always** writes a reply: either the app's response or `{ok:false, error}`.
- **Message envelope:** `{ id: uuid, cmd: string, payload: object }` → `{ id, ok: bool, payload?: object, error?: {code, message} }`.
- **Commands for v1:**
  - `PING` → `{version}`
  - `ADD_DOWNLOAD {url, filename?, referrer?, headers{}, cookies?, filesize?, mime?}` → `{download_id}`
  - `ADD_STREAM {page_url, page_title, playlist_url, headers{}, cookies?}` → `{download_id}`
  - `GET_SUMMARY` → `{active, queued, total_speed_bps}`
  - `PAUSE_ALL`, `RESUME_ALL` → `{}`
  - `SHOW_WINDOW` → `{}`
- Unknown commands and malformed payloads get `ok:false` with a typed error code. They never crash anything.

---

## 5. Download engine (`odm-engine`)

### 5.1 States

`queued → probing → downloading ⇄ paused → completed | failed | cancelled`, plus `merging` for streams. `failed` always carries an `ErrorKind` and a message. There is no separate "error" state.

### 5.2 Lifecycle (one tokio task owns each active download)

1. **Probe.**
   - Send `GET` with `Range: bytes=0-0`, following redirects.
   - `206` with `Content-Range: bytes 0-0/N` means the size is N and ranges are supported.
   - `200` means ranges are not supported. The size comes from `Content-Length` if present, otherwise it is unknown.
   - Record the final URL, `ETag`, `Last-Modified`, the MIME type, and the filename from `Content-Disposition` (RFC 6266, including `filename*`).
   - The probe's response body is dropped straight away.
2. **Plan.**
   - With ranges supported and a known size: create `n = clamp(size / 1 MiB, 1, max_connections)` roughly equal chunks. `max_connections` defaults to 8, with a range of 1–32.
   - Otherwise: one streaming chunk with an open end (`range_end = None`).
3. **Allocate.** Create `<final>.odm-part` and call `set_len(size)` when the size is known.
4. **Workers.**
   - Each chunk sends `GET` with `Range: bytes=<start+written>-<end>` and `If-Range: <etag or last-modified>` when available.
   - In multi-chunk mode, any status other than `206` is a permanent `RangeIgnored` error for that chunk. The download then falls back to a single connection starting from 0.
   - Workers write at their own offset through a positioned write (`seek_write` on Windows) and **never write past `range_end`**.
5. **Dynamic splitting.**
   - When a worker finishes and fewer than `max_connections` are active, it finds the chunk with the most remaining bytes.
   - If that chunk has more than 2 MiB left, it is split at the midpoint of its remaining range, and a new worker takes the upper half.
   - The split is recorded in the store atomically.
6. **Finish.**
   - Check that the total bytes written equal the size (when known).
   - If a checksum (SHA-256/MD5) was supplied, verify it.
   - Flush, close and rename the part file to the final name.
   - Final name collisions resolve to `name (1).ext`, `name (2).ext`, and so on.

### 5.3 Filenames

The name is sanitized:
- Strip `\ / : * ? " < > |` and control characters.
- Remove leading and trailing dots and spaces.
- Refuse `..`.
- Prefix Windows reserved names (`CON`, `NUL`, `COM1`…) with `_`.
- Truncate to 200 characters, keeping the extension.

The save directory comes from the settings or the request, never from path separators inside the filename.

### 5.4 Persistence and resume

- Each chunk row stores `start`, `end?` and `written`.
- Progress is persisted **after** the data it covers has been flushed. This happens at most once per second per download, and always on pause, stop and finish.
- On startup, downloads left in `probing` or `downloading` are reset to `queued`. They resume from each chunk's `start + written`.
- If `If-Range` validation fails on resume (a `200` where a `206` was expected), the server file changed. The chunk data is discarded and the download restarts from zero, with a log entry.

### 5.5 Errors and retries

- `ErrorKind` is split into two groups:
  - **Temporary:** timeout, connection reset or closed, DNS failure, HTTP 408/429/5xx.
  - **Permanent:** other HTTP 4xx, `DiskFull`, `PermissionDenied`, `RangeIgnored` (handled by fallback), `ChecksumMismatch`, `LinkExpired` (403/410 after a download has started).
- Temporary errors are retried per chunk with exponential backoff: 1, 2, 4 … up to 60 s, with ±20% jitter. `Retry-After` is honoured.
- The maximum attempts per chunk is a setting (default 5).
- When a chunk runs out of retries, the download becomes `failed`, and the progress of its other chunks is kept so it can be resumed manually.
- The UI shows `LinkExpired` as "Link expired — re-send it from the browser". A later `ADD_DOWNLOAD` for the same final filename offers to **reuse the existing partial data** with the new URL.

### 5.6 Manager, queue and limits

- The manager is one actor task that owns the queue and receives commands over an `mpsc` channel.
- It starts downloads in priority order, then oldest first, while `active < max_concurrent` (default 3, range 1–10).
- **Speed limit:** a global token bucket in bytes per second (0 means unlimited), with a burst of 1 s.
  - A request can take the bucket negative (debt) and then waits until the debt is repaid. This means **a request larger than the bucket can never block forever**.
  - The limit can be changed at runtime.
  - An optional per-download limit uses a nested bucket.
- **Pause and cancel:** each download has a `CancellationToken`. On pause, workers finish their current write, persist their progress and exit. The download task is `await`ed before it is marked `paused`. Resume is refused until the previous task has fully exited. Cancel can also delete the part file.
- **App shutdown:** cancel everything, persist, and wait at most 3 s.

### 5.7 Progress events

- The engine publishes `Progress {id, bytes, total?, speed_bps, eta_s?, chunks: [{start, end?, written, state}]}`.
- `speed_bps` is a **moving average over 5 s** of bytes received in this session. Resumed bytes are excluded.
- The Tauri layer throttles events to 4 per second per download and batches them into one `progress-batch` event.

### 5.8 Categories

- A download's category is decided by extension first, then MIME type. The mapping is carried over from the old project: video, audio, document, compressed, image, program (new: `.exe .msi`), and general.
- Each category maps to a subfolder under the download directory, and the mapping is configurable.

---

## 6. HLS (`odm-engine::hls`)

- **Input:** a playlist URL plus headers and cookies.
- **Master playlist:** parse the `#EXT-X-STREAM-INF` variants. Pick the highest bandwidth by default, or the user's choice. The variant list is kept for the UI.
- **Media playlist:**
  - Read the segments and `#EXT-X-MAP` (fMP4 init segment).
  - For `#EXT-X-KEY METHOD=AES-128`, fetch the key once and decrypt with AES-128-CBC. The IV comes from the tag or, if absent, from the media sequence number.
  - `SAMPLE-AES` is unsupported, and the download fails with that reason.
- **Live playlists** (no `#EXT-X-ENDLIST`) are unsupported in v1 and fail with a clear message.
- **Download:**
  - Segments are fetched into `<save_dir>\.odm-hls-<id>\seg_000001.ts` (6-digit, 1-based).
  - Up to `max_connections` segments download in parallel, using the same retry and speed-limiter code as file downloads.
  - Segment state is stored in the database, so streams can resume.
- **Merge:**
  - State becomes `merging`.
  - Write an FFmpeg concat list, then run `ffmpeg -hide_banner -y -f concat -safe 0 -i list.txt -c copy -movflags +faststart out.mp4`.
  - A non-zero exit code means `failed` with FFmpeg's last 20 stderr lines. On success, delete the temp directory.
  - If FFmpeg is missing, the download fails with "FFmpeg not found — set its path in Settings".
- **Progress:** measured as completed segments over total segments. Bytes and speed are measured the same way as for file downloads.

---

## 7. Storage (`odm-store`)

- SQLite in WAL mode with `foreign_keys=ON`.
- **One dedicated thread owns the connection.** Other code sends it requests over a channel, so no connection is ever shared between threads.
- **Migrations:** `PRAGMA user_version` plus an ordered list of embedded SQL files. They run at startup inside a transaction.
- **Tables:**
  - `downloads`: id, url, final_url, filename, save_dir, size?, mime, category, state, error_kind?, error_msg?, priority, max_connections, speed_limit?, etag?, last_modified?, checksum?, kind (`file` | `hls`), page_url?, page_title?, created_at, started_at?, finished_at?
  - `download_headers`: download_id, name, value. This includes cookies. The table is stored locally only and never logged.
  - `chunks`: download_id, idx, start, end?, written, state
  - `hls_segments`: download_id, idx, url, duration_ms, state, bytes
  - `hls_meta`: download_id, variant_url, key_url?, iv?, map_url?
- **Settings** live in `settings.json`, written atomically (temp file then rename). Unknown or invalid fields fall back to defaults with a log warning.

---

## 8. Browser extension

- **Download capture:**
  - Chromium uses `downloads.onDeterminingFilename`; Firefox uses `downloads.onCreated`.
  - A download is skipped if:
    - the URL isn't http(s);
    - the size is known and below `min_capture_bytes` (default 1 MB);
    - Alt was held (a content script tracks the modifier and messages the background);
    - its host is on the user's skip list;
    - capture is turned off.
  - Otherwise the extension collects the cookies for the URL (`cookies.getAll`), the Referer (the tab URL) and the User-Agent, then sends `ADD_DOWNLOAD`.
  - **The browser's download is cancelled only after ODM replies `ok:true`.** On failure or a 5 s timeout, the browser download continues untouched.
- **Context menu:** "Download with ODM" on links and on media elements.
- **Video sniffing:**
  - `webRequest.onHeadersReceived` watches for `.m3u8` URLs or the content types `application/vnd.apple.mpegurl` / `application/x-mpegURL`. It also detects `.mpd` URLs and `application/dash+xml`, which are marked as unsupported.
  - Findings are kept per tab (deduplicated, cleared on navigation) and shown as a badge count.
  - A small, dismissible in-page button appears **only when the tab has at least one detected stream**, and it lists them.
  - The popup lists them too. Clicking one sends `ADD_STREAM`. **Nothing is ever sent automatically.**
- **Popup:** connection status (`PING`), active/queued count, total speed, the detected streams, Pause all / Resume all, and Open ODM.
- **Registration:**
  - `odm-host --register` writes the host manifest JSON next to `odm-host.exe`.
  - It sets HKCU keys for Chrome, Edge, Brave, Vivaldi, Opera, Chromium, Firefox and Floorp (Floorp uses the Mozilla key path).
  - `allowed_origins` is `chrome-extension://<fixed id>/` and `allowed_extensions` is `odm@odm.local`.
  - ODM runs the registration on first start, and whenever the stored host path differs from its current location.

---

## 9. UI (Svelte in Tauri)

- **Layout:**
  - Left sidebar: All, Active, Queued, Completed, Failed, then each category, all with counts.
  - Main area: the download table, with columns for name, size, progress bar, speed, ETA, status and added date/time.
  - Toolbar: Add, Batch add, Resume, Pause, Cancel, Remove (with a "delete file" checkbox), and Settings.
- **Detail panel** (double-click or Enter):
  - The chunk map: one segment per chunk, filled in proportion to its progress and coloured by state.
  - Also shows the URL, final path, size, speed, ETA, attempts, the last error, and header names (values hidden).
  - Actions: Open file, Open folder, Copy URL, and Refresh URL (paste a new link for expired downloads).
- **Add dialog:** URL, filename (auto-filled after the probe), folder, connections, and category. For HLS it also shows a variant picker.
- **Batch add:** paste one URL per line. It shows how many are valid, then adds them all.
- **Settings:** folders and category mapping, max concurrent downloads, connections per download, global speed limit, retries, timeouts, FFmpeg path, capture on/off, minimum capture size, skip-hosts list, close to tray, start minimized, and notifications.
- **Tray and window:**
  - A real icon, and the tray menu has Show, Pause all, Resume all and Quit.
  - Closing the window hides it to the tray when enabled. Quit asks for confirmation if downloads are active.
  - Only one instance runs. A second launch focuses the existing window.
  - When a download finishes, a Windows notification appears; clicking it opens the file's folder.
- **Theme:** dark, with indigo accents taken from the old `styles.qss` (`#0f0f14` background, `#6366f1` accent).

---

## 10. Logging and privacy

- `tracing` writes to `%APPDATA%\ODM\logs\odm.log`, rotating daily and keeping 7 files.
- Cookie and Authorization header values are **never logged**. URLs are logged without query strings.

---

## 11. Testing

- **Unit tests:**
  - chunk planning and splitting;
  - filename sanitizing and collision naming;
  - `Content-Disposition` parsing;
  - the token bucket, including a request larger than the bucket;
  - backoff timing;
  - the state machine's allowed transitions;
  - the m3u8 parser (master, media, keys, map, live);
  - IPC framing, including oversize and truncated messages.
- **Integration tests** use an in-process test HTTP server (`axum`) that can:
  - serve with or without range support;
  - omit the size (chunked encoding);
  - drop a connection after N bytes;
  - ignore `Range` headers;
  - return 429 with `Retry-After`, or 503;
  - change the ETag partway through;
  - redirect;
  - return 403 on resume.
- **Required regression tests** (from the old audit):
  - a download with unknown size completes;
  - a server that ignores Range still produces a correct file;
  - a speed limit below the buffer size doesn't hang;
  - 32 concurrent downloads × 32 connections don't deadlock;
  - an immediate pause→resume never produces two writers;
  - killing the process mid-download and resuming gives an identical SHA-256;
  - the part file has its full size before any writes.
- **HLS tests:** a local test server serves the playlist and segments, including AES-128 encryption. The merge is tested with the installed FFmpeg, and that test is skipped if FFmpeg is absent.
- **Host test:** spawn `odm-host`, feed it framed stdin, and assert the stdout replies against a fake pipe server.
- **Manual smoke checklist** (in the plan): capture in Chrome and Firefox, the cookie-gated download, the Alt-bypass, the context menu, and stream detection on a public HLS test page.

---

## 12. Build phases (each ends with something working)

0. **Toolchain and skeleton:** install rustup and the VS Build Tools; create the workspace, crates, Tauri app and extension scaffold; CI-less `cargo test` + `pnpm test` pass.
1. **Engine and store:** the full download engine with its tests, driven by `odm-cli get <url>`.
2. **Tauri UI:** the table, the detail panel with the chunk map, the dialogs, settings, tray and notifications.
3. **Native host and extension:** capture, context menu, popup, registration.
4. **HLS:** detection in the extension, then download, decrypt and merge in the engine.
5. **Polish:** a Tauri installer bundle for personal use (MSI/NSIS, unsigned), first-run registration, and README.
