# ODM Plan 4: HLS Video Streams Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An HLS video found on a page (or pasted into the Add dialog) downloads in parallel, resumes after a pause or crash, is decrypted when it uses AES-128, and comes out as a playable `.mp4`.

**Architecture:**
- **Parser:** `odm-engine::hls` has a pure m3u8 parser and an AES-128-CBC decryptor.
- **Downloading:** `run_hls` downloads segments into `<save_dir>\.odm-hls-<id>\`, with segment state in the store. It then runs FFmpeg to merge them.
- **Manager:** it launches `run_hls` for `kind = "hls"` records and uses the same queue, limiter, retry rules and events as file downloads.
- **App:** the appcore dispatcher handles `ADD_STREAM`, and the Add dialog shows a variant picker.
- **Extension:** it watches response headers for playlists, keeps them per tab, and shows them in the popup and in a small in-page button. **Nothing is sent automatically.**

**Tech Stack:**
- Existing: Rust, tokio, reqwest, rusqlite, Svelte 5, TypeScript.
- New: `aes = "0.8"` and `cbc = "0.1"` (with the `alloc` and `block-padding` features).
- FFmpeg 9.0 (external, found through `ffmpeg_path` or PATH).

**Spec:** `docs/superpowers/specs/2026-09-29-odm-design.md`, sections 6, 7 (`hls_segments`, `hls_meta`), 8 (video sniffing), 9 (variant picker) and 11 (HLS tests).

## Decisions beyond the spec (binding)

1. **Merging:**
   - TS segments use the spec's concat list. `list.txt` sits in the temp dir with bare file names (`file 'seg_000001.ts'`), and FFmpeg runs with that dir as its working directory, so paths never need escaping.
   - fMP4 segments (`#EXT-X-MAP`) are first joined byte-for-byte (`init.mp4` + segments → `joined.mp4`), then remuxed with `ffmpeg -hide_banner -y -i joined.mp4 -c copy -movflags +faststart out.mp4`. A concat list of fMP4 fragments without their init segment does not play.
2. **Keys and IVs are not stored.**
   - Each run re-fetches the media playlist (`hls_meta.variant_url`) and re-parses it.
   - Stored segment rows carry the state and bytes only.
   - If the re-parsed segment list no longer matches the stored URLs, segment progress is discarded and the download starts over, with a log line.
3. **Progress:**
   - `ProgressSnapshot` gains `segments: Option<SegmentProgress { done, total }>`, which is omitted from the JSON when `None`.
   - For HLS, `bytes` is the number of segment bytes on disk.
   - `total` is the estimate `bytes / done * total`, available once at least one segment is done.
   - The UI uses `segments` for the percentage when present.
4. **HLS failures** use `ErrorKind::Other` with exact plain-language messages. The UI already shows `Other` messages as they are:
   - `"Live streams are not supported yet."`
   - `"This stream uses SAMPLE-AES encryption, which ODM can't download."`
   - `"FFmpeg not found — set its path in Settings."`
   - `"FFmpeg could not merge the video:\n<last 20 stderr lines>"`
   - `"The playlist has no video segments."`
5. **Output name:**
   - the request's filename (sanitised, `.mp4` enforced);
   - else the sanitised `page_title` + `.mp4`;
   - else the playlist file stem + `.mp4`;
   - else `video.mp4`.
   The name is made unique at finish, like files.
6. **Extension detection:** every unique playlist URL per tab is listed, including variant playlists of a master. The app accepts both. The list is labelled with the URL's last path segment and its host.

## Global Constraints

- Everything in Plan 3's Global Constraints still holds: privacy, identity, the gates and the git rules.
- Segment files are named `seg_000001.ts` (6 digits, 1-based) in `<save_dir>\.odm-hls-<id>\`. fMP4 segments use the extension `.m4s`, and the init segment is `init.mp4`.
- Parallelism is up to the record's `max_connections` segments at once. Segments use the shared `SpeedLimiter`, `backoff_delay`, `max_attempts` and `classify_status(status, started = true)` (so 403/410 count as `LinkExpired`).
- The FFmpeg command for TS is exactly `ffmpeg -hide_banner -y -f concat -safe 0 -i list.txt -c copy -movflags +faststart out.mp4`.
- FFmpeg lookup:
  - `EngineConfig.ffmpeg_path` if set and the file exists;
  - else the first `ffmpeg.exe` on `PATH`;
  - else the "FFmpeg not found" failure.
  It is never bundled.
- The FFmpeg child runs with `CREATE_NO_WINDOW` (0x0800_0000) and is killed if the download is cancelled.
- On success, the temp dir is deleted. Cancel and Remove of an unfinished HLS download also delete it.
- HLS tests that need FFmpeg skip themselves (with `eprintln!("skipped: ffmpeg not found")`) when it isn't found.

## Review Focus

1. **A relative URI everywhere** (variant, segment, key, map, including `../` and absolute-path forms). Each resolves against the playlist that contains it. *Test: Task 1 `relative_uris_resolve_against_their_playlist`.*
2. **A pause during a segment, then resume.** Finished segments are not fetched again; a partial segment is fetched again whole. The merged file plays. *Test: Task 3 `pause_resume_skips_done_segments`.*
3. **An FFmpeg failure** (corrupt segments). The download fails with FFmpeg's stderr tail, and the temp dir stays so a retry can reuse it. *Test: Task 4 `corrupt_segments_fail_with_ffmpeg_output`.*
4. **A playlist with no `#EXT-X-ENDLIST`** (live), and SAMPLE-AES. Each fails at once with its message, and no segment is fetched. *Test: Task 3 `live_and_sample_aes_fail_fast`.*
5. **A page that loads many playlists** (master + variants + subtitle playlists). The extension's list is deduplicated, cleared on navigation, and never sends anything by itself. *Test: Task 6 `stream_store_dedupes_and_clears`.*

---

### Task 1: m3u8 parser and AES-128 decryption

**Files:**
- Create: `crates/odm-engine/src/hls/mod.rs`, `crates/odm-engine/src/hls/playlist.rs`, `crates/odm-engine/src/hls/crypto.rs`
- Modify: `crates/odm-engine/src/lib.rs` (`pub mod hls;`), workspace and engine `Cargo.toml` (`aes`, `cbc`)
- Test: unit tests in `playlist.rs` and `crypto.rs`

**Interfaces:**
```rust
// playlist.rs
#[derive(Debug, Clone, PartialEq)] pub struct Variant { pub url: String, pub bandwidth: u64, pub resolution: Option<String>, pub codecs: Option<String> }
#[derive(Debug, Clone, PartialEq)] pub enum KeyMethod { None, Aes128, SampleAes, Other(String) }
#[derive(Debug, Clone, PartialEq)] pub struct Key { pub method: KeyMethod, pub uri: Option<String>, pub iv: Option<[u8; 16]> }
#[derive(Debug, Clone, PartialEq)] pub struct Segment { pub url: String, pub duration_ms: u64, pub seq: u64, pub key: Option<Key>, pub byte_range: Option<(u64, Option<u64>)> }
#[derive(Debug, Clone, PartialEq)] pub struct MediaPlaylist { pub segments: Vec<Segment>, pub map_url: Option<String>, pub ended: bool, pub media_sequence: u64 }
#[derive(Debug, Clone, PartialEq)] pub enum Playlist { Master(Vec<Variant>), Media(MediaPlaylist) }
#[derive(Debug, PartialEq)] pub struct ParseError(pub String);
/// `base` is the playlist's own URL; every URI is returned absolute.
pub fn parse(text: &str, base: &str) -> Result<Playlist, ParseError>;
/// Highest bandwidth; ties keep the first.
pub fn best_variant(v: &[Variant]) -> Option<&Variant>;
// crypto.rs
pub fn iv_for(key: &Key, seq: u64) -> [u8; 16];          // key.iv, else seq as 128-bit big-endian
pub fn decrypt_aes128(key: &[u8; 16], iv: &[u8; 16], data: &[u8]) -> Result<Vec<u8>, String>; // CBC + PKCS7
```
**Parser rules:**
- The text must start with `#EXTM3U` (an optional BOM and leading whitespace are allowed), else `ParseError("not an HLS playlist")`.
- `#EXT-X-STREAM-INF` attributes are parsed with quoted-string support (`CODECS="avc1,mp4a"` contains a comma). The next non-comment line is the variant URI.
- `#EXTINF:<sec>` gives the duration, rounded to ms. `#EXT-X-MEDIA-SEQUENCE` defaults to 0. Segment `seq` = media sequence + index.
- `#EXT-X-KEY` applies to the following segments until the next `#EXT-X-KEY`. `METHOD=NONE` → `key: None`. `IV=0x…` is parsed as 16 bytes, left-padded.
- `#EXT-X-MAP:URI=…` sets `map_url`. `#EXT-X-BYTERANGE` sets `byte_range` on the next segment. Byte ranges are parsed but not supported for download: Task 3 fails with `"Byte-range playlists are not supported yet."`.
- `#EXT-X-ENDLIST` sets `ended`. Unknown tags are ignored. CRLF line endings are accepted.

- [ ] **Step 1: Write the failing tests:**
  - `parses_master_with_quoted_codecs`: two variants, bandwidths 800000 and 2500000, `resolution "1280x720"`, codecs `"avc1.4d401f,mp4a.40.2"`. `best_variant` is the 2.5 Mbit one.
  - `parses_media_with_aes_key_and_map`: three segments with durations 4.0, 4.0 and 2.5 s → 4000/4000/2500 ms. Media sequence 7 → seqs 7, 8, 9. All share `Key{Aes128, uri: Some(abs), iv: None}`. `map_url` is absolute and `ended` is true.
  - `key_none_and_rotation`: segments 1–2 use key A, then `METHOD=NONE` for segment 3, then key B with `IV=0x1` for segment 4. The `iv` is `[0;15] + [1]`.
  - `relative_uris_resolve_against_their_playlist` (Review Focus 1): base `https://cdn.example/a/b/index.m3u8` resolves:
    - `seg1.ts` → `https://cdn.example/a/b/seg1.ts`;
    - `../c/seg2.ts` → `https://cdn.example/a/c/seg2.ts`;
    - `/root/seg3.ts` → `https://cdn.example/root/seg3.ts`;
    - the key URI `key.bin` → `https://cdn.example/a/b/key.bin`;
    - an absolute `https://other/x.ts` stays as it is.
  - `live_playlist_has_no_endlist`: `ended == false`.
  - `rejects_non_playlist`: `"<html>"` → `Err`.
  - `crlf_and_bom_accepted`.
  - `crypto.rs` `decrypt_round_trip`: encrypt 1000 bytes with `cbc::Encryptor<aes::Aes128>` and PKCS7 in the test, then `decrypt_aes128` returns the original. A wrong key returns `Err` (bad padding) or different bytes; assert `!= original`.
  - `iv_from_sequence`: `iv_for(&Key{iv: None, ..}, 5)` == `[0;15] + [5]`.
- [ ] **Step 2: Run** `cargo test -p odm-engine hls`. Expected: compile failure.
- [ ] **Step 3: Implement.** Resolve URIs with `reqwest::Url::join`.
- [ ] **Step 4: Run.** Expected: pass. Then run the Rust gates.
- [ ] **Step 5: Commit**
  - Message: `feat(engine): HLS playlist parser and AES-128 decryption`

---

### Task 2: Store support and an HLS test server

**Files:**
- Modify:
  - `crates/odm-store/src/models.rs`: add `SegmentRecord`, `HlsMeta`, `DownloadRecord::new_hls`
  - `crates/odm-store/src/repo.rs`, `crates/odm-store/src/lib.rs`
  - `crates/odm-testserver/src/lib.rs`: add `pub mod hls;`
- Create: `crates/odm-testserver/src/hls.rs`
- Test: `crates/odm-store/tests/store.rs`, `crates/odm-engine/tests/testserver_smoke.rs`

**Interfaces:**
```rust
pub struct SegmentRecord { pub idx: u32, pub url: String, pub duration_ms: u64, pub state: String /* "queued" | "done" */, pub bytes: u64 }
pub struct HlsMeta { pub variant_url: String, pub key_url: Option<String>, pub iv: Option<String>, pub map_url: Option<String> }
impl DownloadRecord { pub fn new_hls(id: &str, playlist_url: &str, filename: &str, save_dir: &str) -> Self; } // kind = "hls"
impl Store {
    pub async fn replace_segments(&self, id: &str, s: Vec<SegmentRecord>) -> Result<(), StoreError>;   // one transaction
    pub async fn get_segments(&self, id: &str) -> Result<Vec<SegmentRecord>, StoreError>;               // by idx
    pub async fn mark_segment_done(&self, id: &str, idx: u32, bytes: u64) -> Result<(), StoreError>;
    pub async fn set_hls_meta(&self, id: &str, m: HlsMeta) -> Result<(), StoreError>;                   // upsert
    pub async fn get_hls_meta(&self, id: &str) -> Result<Option<HlsMeta>, StoreError>;
}
```
- The byte count used by `list_with_bytes` / `get_with_bytes` (the `BYTES` expression) becomes: the chunk sum + `COALESCE((SELECT SUM(bytes) FROM hls_segments WHERE download_id = downloads.id AND state = 'done'), 0)`.
- `odm_testserver::hls::HlsServer`:
  ```rust
  #[derive(Clone, Default)] pub struct HlsFaults { pub bytes_per_sec: Option<u64>, pub fail_segment_once: Option<u32> /* idx (1-based) answered 503 once */ }
  pub struct HlsServer { pub base: String, pub segment_requests: Arc<AtomicU32> }
  impl HlsServer {
      /// Serves files from `dir` at "/<relative path>" (Content-Type from the extension: .m3u8 → application/vnd.apple.mpegurl,
      /// .ts → video/mp2t, .m4s/.mp4 → video/mp4, else application/octet-stream). 404 for missing files.
      pub async fn start(dir: PathBuf, faults: HlsFaults) -> HlsServer;
      pub fn url(&self, path: &str) -> String;
  }
  /// Writes a synthetic VOD playlist "media.m3u8" with `n` segments "s1.ts".."s<n>.ts" of `seg_len` pattern bytes each
  /// (byte i of segment k = (k*31 + i) % 251), optionally AES-128 encrypted with key file "key.bin" (16 bytes 0..15) and IV from sequence.
  /// Returns the plaintext of every segment, in order.
  pub fn write_synthetic(dir: &Path, n: u32, seg_len: usize, encrypt: bool) -> Vec<Vec<u8>>;
  /// Uses ffmpeg (if found on PATH) to write a real 4-second VOD stream "real/index.m3u8" with 1-second TS segments;
  /// returns false when ffmpeg is unavailable.
  pub fn write_real_ts(dir: &Path) -> bool;
  ```
  - `write_real_ts` runs: `ffmpeg -hide_banner -y -f lavfi -i testsrc=duration=4:size=160x120:rate=10 -f lavfi -i sine=duration=4 -c:v libx264 -pix_fmt yuv420p -c:a aac -f hls -hls_time 1 -hls_playlist_type vod real/index.m3u8`.
  - The test server's dev-dependency on `aes`/`cbc` is fine; the testserver crate is test-only.

- [ ] **Step 1: Write the failing tests:**
  - Store: `segments_round_trip_and_bytes`. Insert an hls record and 3 segments, then `mark_segment_done(id, 2, 500)`. `get_segments` shows `[queued, done, queued]`, and `get_with_bytes` gives 500.
  - Store: `hls_meta_upsert`. Setting twice keeps the last value.
  - Smoke: `hls_server_serves_synthetic_playlist`. `write_synthetic(dir, 3, 1000, true)`, then GET `/media.m3u8` returns 200 with content type `application/vnd.apple.mpegurl` and a body containing `#EXT-X-KEY:METHOD=AES-128`. GET `/s1.ts` decrypted with `decrypt_aes128` equals plaintext 0. GET `/nope` returns 404.
- [ ] **Step 2: Run.** Expected: failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p odm-store -p odm-engine --test testserver_smoke`. Expected: pass. Then run the Rust gates.
- [ ] **Step 5: Commit**
  - Message: `feat(store): HLS segment storage; HLS test server`

---

### Task 3: Engine: downloading HLS segments

**Files:**
- Create: `crates/odm-engine/src/hls/run.rs`
- Modify:
  - `crates/odm-engine/src/hls/mod.rs`
  - `crates/odm-engine/src/manager.rs`: `add_stream`; `launch` dispatches on `rec.kind`
  - `crates/odm-engine/src/events.rs`: `SegmentProgress`
  - `crates/odm-engine/src/config.rs`: `ffmpeg_path: Option<PathBuf>`, default `None`
- Test: `crates/odm-engine/tests/hls.rs`

**Interfaces:**
```rust
// events.rs
#[derive(Debug, Clone, Copy, PartialEq, Serialize)] pub struct SegmentProgress { pub done: u32, pub total: u32 }
// ProgressSnapshot gains: #[serde(skip_serializing_if = "Option::is_none")] pub segments: Option<SegmentProgress>
// manager.rs
#[derive(Debug, Clone, Default)]
pub struct StreamRequest { pub playlist_url: String, pub variant_url: Option<String>, pub filename: Option<String>,
    pub save_dir: Option<PathBuf>, pub page_url: Option<String>, pub page_title: Option<String>,
    pub headers: Vec<(String, String)>, pub start_paused: bool }
impl ManagerHandle { pub async fn add_stream(&self, req: StreamRequest) -> Result<String, OdmError>; }
// hls/run.rs
pub async fn run_hls(env: &DownloadEnv, id: &str, cancel: CancellationToken) -> Outcome;
pub fn temp_dir(rec: &DownloadRecord) -> PathBuf;   // <save_dir>/.odm-hls-<id>
```
- `add_stream`:
  - It checks the scheme like `add`.
  - It inserts `new_hls` with category `video`. The save dir is the request's, or `config.save_dir_for("video")`.
  - The filename follows decision 5. Until the merge it is a provisional name.
  - It sets `hls_meta.variant_url` = `variant_url` or `playlist_url`, then emits `Added` and schedules.
- The existing `EngineEvent` JSON snapshot test is updated **only** by adding the omitted-when-None field. Add one new snapshot case with `segments: Some`.

**`run_hls` download phase:**
1. Transition to Probing.
2. GET the `variant_url` with the stored headers:
   - non-success → the classified error;
   - `parse` error → Other `"The playlist could not be read."`;
   - `Master(v)` → `best_variant`, then store it as the new `variant_url` and fetch again (one level only).
3. Check, failing fast in this order:
   - not `ended` → the live message;
   - any segment key `SampleAes` → the SAMPLE-AES message;
   - any `Other` key method → Other `"This stream's encryption is not supported."`;
   - any `byte_range` → the byte-range message;
   - zero segments → the no-segments message.
4. Compare with the stored segments:
   - none stored → `replace_segments` (all queued);
   - stored URLs differ from the parsed ones → `replace_segments` and `remove_dir_all` of the temp dir, with a log line.
5. Transition to Downloading. Create the temp dir. Download `map_url` to `init.mp4` if it is missing.
6. Process the segments whose state isn't `done`, or whose file is missing or of a different size than the stored `bytes`, in a `JoinSet` limited to `rec.max_connections`. For each segment:
   - GET with the headers (one GET for the whole body; HLS segments are small);
   - acquire the limiter per received piece;
   - decrypt with the key (fetched once per key URI per run and cached; it must be 16 bytes, else Other `"The stream's key could not be read."`) and `iv_for`;
   - write `seg_NNNNNN.<ts|m4s>.part`, rename it to the final name, then `mark_segment_done`.
   - Transient errors are retried with `backoff_delay` up to `max_attempts`; the counter resets per segment. Permanent errors → the download fails.
7. Emit `Progress` at `config.progress_interval`:
   - bytes on disk;
   - `total` estimate, `speed_bps` (5 s moving average), `eta_s`;
   - `chunks: vec![]`;
   - `segments: Some{done, total}`.
8. Cancellation: stop starting new segments and wait for the in-flight ones to end (each checks `cancel` between pieces and deletes its `.part`). Return `Stopped`.
9. When every segment is done → the merge phase (Task 4). In this task, return `Outcome::Failed(Other "merge not implemented")`; Task 4 replaces it.

**Manager:** `launch` reads the record's kind (`env.store.get_download`) inside the spawned task and calls `run_hls` for `"hls"`, otherwise `run_download`.

- [ ] **Step 1: Write the failing tests** in `tests/hls.rs`. Each uses `HlsServer` with `write_synthetic` in a tempdir, a manager on an in-memory store, and the download dir in a tempdir.
  - `downloads_and_decrypts_all_segments`:
    - `write_synthetic(dir, 12, 50_000, true)`, then `add_stream(media.m3u8)`.
    - Wait for state Failed with the message `"merge not implemented"`.
    - Every `seg_0000NN.ts` in `temp_dir` equals the plaintext.
    - The store shows 12 done, and `get_with_bytes` = 600 000.
  - `master_playlist_picks_best_variant`: write a `master.m3u8` pointing at `low.m3u8` (bandwidth 1) and `media.m3u8` (bandwidth 9). After the run, `hls_meta.variant_url` ends with `media.m3u8`.
  - `live_and_sample_aes_fail_fast` (Review Focus 4):
    - A playlist without `ENDLIST` → Failed with the exact live message, and `segment_requests == 0`.
    - A playlist with `METHOD=SAMPLE-AES` → its message.
  - `pause_resume_skips_done_segments` (Review Focus 2):
    - 20 segments × 100 000 bytes, with `bytes_per_sec: Some(400_000)`.
    - Pause once at least 5 are done; wait for Paused, then record `done_before`.
    - Resume and wait for the terminal state ("merge not implemented").
    - `segment_requests <= 20 + max_connections` (only partial segments are fetched again), and every file equals its plaintext.
  - `transient_segment_error_is_retried`: `fail_segment_once: Some(3)` still downloads all segments.
  - `progress_reports_segments`: a captured `Progress` event has `segments.total == 12` and `chunks` empty.
- [ ] **Step 2: Run** `cargo test -p odm-engine --test hls`. Expected: compile failure.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p odm-engine` three times. Expected: pass. Then run the Rust gates.
- [ ] **Step 5: Commit**
  - Message: `feat(engine): download HLS segments in parallel with resume and AES-128`

---

### Task 4: Engine: merging with FFmpeg, finishing, cleanup

**Files:**
- Create: `crates/odm-engine/src/hls/merge.rs`
- Modify: `crates/odm-engine/src/hls/run.rs`, `crates/odm-engine/src/download.rs` (expose the finish helper), `crates/odm-engine/src/manager.rs` (cancel/remove clean the HLS temp dir)
- Test: `crates/odm-engine/tests/hls.rs`; unit tests in `merge.rs`

**Interfaces:**
```rust
pub fn find_ffmpeg(configured: Option<&Path>) -> Option<PathBuf>;   // configured (if it exists) → PATH search for ffmpeg.exe
pub fn concat_list(n: u32, ext: &str) -> String;                     // "file 'seg_000001.ts'\n..." (LF)
/// Runs ffmpeg in `dir`, producing dir/out.mp4. fmp4 = true joins init.mp4 + segments into joined.mp4 first.
pub async fn merge(ffmpeg: &Path, dir: &Path, n: u32, fmp4: bool, cancel: &CancellationToken) -> Result<PathBuf, OdmError>;
/// Last `n` non-empty lines of `stderr`.
pub fn stderr_tail(stderr: &str, n: usize) -> String;
// download.rs: the existing "unique name + rename under FINISH_LOCK + Mark of the Web" logic becomes
pub(crate) fn place_final(src: &Path, dir: &Path, name: &str, rec: &DownloadRecord) -> Result<PathBuf, OdmError>;
```
**Merge phase** (replaces Task 3's placeholder):
1. Transition to Merging.
2. `find_ffmpeg(config.ffmpeg_path)`: if none, fail with the FFmpeg-not-found message. The temp dir is kept.
3. `merge`:
   - it uses `tokio::process::Command` with `CREATE_NO_WINDOW` and `kill_on_drop(true)`;
   - on cancel, it kills the child and returns Cancelled;
   - a non-zero exit → the "could not merge" message with `stderr_tail(…, 20)`; the temp dir is kept.
4. `place_final(dir/out.mp4, rec.save_dir, name, rec)`.
5. Set the record's size to the output file length (`set_placement` or a new small `set_size`; the implementer chooses).
6. `remove_dir_all(temp)`, then return `Completed{path}`.

**Manager:** in `cancel` and `remove`, for a record with kind `"hls"` that is not completed, `remove_dir_all(temp_dir(rec))` after the run task has finished. This mirrors the part-file handling.

- [ ] **Step 1: Write the failing tests:**
  - Unit tests:
    - `concat_list(2, "ts") == "file 'seg_000001.ts'\nfile 'seg_000002.ts'\n"`.
    - `stderr_tail("a\n\nb\nc\n", 2) == "b\nc"`.
    - `find_ffmpeg(Some(Path::new("Z:/nope/ffmpeg.exe")))` falls back to PATH; the test only asserts that it doesn't panic.
  - `real_stream_merges_to_playable_mp4` (skips without FFmpeg):
    - `write_real_ts(dir)`, serve it, `add_stream(real/index.m3u8)` with `page_title: Some("Test: clip?")`.
    - It completes. The output is `Test clip.mp4` (sanitised) in the save dir and larger than 10 KB.
    - `ffprobe -v error -show_entries format=duration -of csv=p=0 <out>` prints a duration between 3.5 and 4.5. If `ffprobe` is missing, skip that assertion.
    - The temp dir is gone.
  - `encrypted_real_stream_merges`:
    - Encrypt the files from `write_real_ts` in the test with key 0..15 and a sequence IV, rewriting `index.m3u8` with `#EXT-X-KEY:METHOD=AES-128,URI="key.bin"` after `#EXT-X-TARGETDURATION`.
    - It completes with a playable duration (skip without FFmpeg).
  - `missing_ffmpeg_fails_with_message`: with config `ffmpeg_path = Some("Z:/nope/ffmpeg.exe")` **and** `PATH` not consulted, the result is Failed with exactly `"FFmpeg not found — set its path in Settings."`, and the temp dir still holds the segments.
    - Add `EngineConfig.ffmpeg_search_path: bool` (default `true`) so that tests can turn off the PATH search. The name is binding.
  - `corrupt_segments_fail_with_ffmpeg_output` (Review Focus 3): `write_synthetic` with pattern bytes, which are not valid TS. With FFmpeg present, the result is Failed with a message starting `"FFmpeg could not merge the video:"`, and the temp dir is kept.
  - `cancel_removes_temp_dir`: cancel a throttled HLS download, and the temp dir is gone.
- [ ] **Step 2: Run.** Expected: failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p odm-engine` three times. Expected: pass, with real-FFmpeg tests running on this machine (FFmpeg 9.0 is installed). Then run the Rust gates.
- [ ] **Step 5: Commit**
  - Message: `feat(engine): merge HLS with FFmpeg into mp4`

---

### Task 5: App: ADD_STREAM, variant picker, FFmpeg setting, segment progress

**Files:**
- Modify:
  - `crates/odm-appcore/src/ipc.rs` (`ADD_STREAM`), `crates/odm-appcore/src/api.rs` (`suggest`, `add`), `crates/odm-appcore/src/settings.rs` (`to_engine_config` passes `ffmpeg_path`)
  - `app/src/lib/types.ts`, `app/src/lib/model.ts`, `app/src/components/AddDialog.svelte`, `app/src/components/DetailPanel.svelte`, `app/src/components/DownloadTable.svelte` (or wherever the percentage and status are rendered)
- Test: `crates/odm-appcore/tests/ipc.rs`, `crates/odm-appcore/tests/api.rs`, `app/src/lib/model.test.ts`

**Interfaces:**
- `ADD_STREAM`:
  - Parse `AddStream`; skip filters never apply (it is always an explicit click).
  - Headers: `payload.headers` + `Referer = page_url` (unless given) + `Cookie`, passed through `normalize_headers`.
  - Call `manager.add_stream(StreamRequest{ playlist_url, page_url: Some, page_title, headers, .. })` and reply `Added{download_id}`. Replace Plan 3's `not_supported` reply.
- `api::Suggestion` gains `variants: Vec<VariantDto>` (empty for plain files) and `is_stream: bool`:
  ```rust
  #[derive(Serialize)] pub struct VariantDto { pub url: String, pub bandwidth: u64, pub resolution: Option<String>, pub label: String }
  ```
  - `label` = `"1280×720 · 2.5 Mbit/s"`, or `"2.5 Mbit/s"` when there is no resolution. Bandwidth / 1e6 with one decimal; values under 1 Mbit are shown as kbit/s with no decimal.
  - `suggest(url)`: when the URL path ends in `.m3u8` (case-insensitive), or the probe's MIME is an HLS type:
    - fetch and `parse` the playlist;
    - master → variants sorted by bandwidth, descending; media → empty variants;
    - `is_stream = true`, `filename` = decision 5 using the playlist stem, `category "video"`, `size None`, `resumable true`.
- `api::AddArgs` gains `variant_url: Option<String>` and `stream: bool`. When `stream`, `api::add` calls `add_stream`.
- `Settings::to_engine_config` sets `ffmpeg_path = Some(path)` when it is non-empty.
- **UI:**
  - When `suggestion.is_stream`, the Add dialog shows "Video stream" and a `<select>` labelled "Quality" listing the variant labels (the first is selected, the highest). The connections field stays. Add sends `stream: true` and `variant_url`.
  - `model.ts`: `export function progressPct(row): number`. It uses `segments.done / segments.total` when the last progress has `segments`, else `bytes / size`; in both cases clamped to 0–100.
  - The row's status label for `merging` is "Merging".
  - The detail panel shows "Segments: {done} of {total}" in place of the chunk map for HLS rows. `kind` is added to `DownloadDto` if it isn't there yet; check `dto.rs`.

- [ ] **Step 1: Write the failing tests:**
  - `ipc.rs` `add_stream_adds_hls_download`: `HlsServer` with a synthetic playlist, then `ADD_STREAM{page_url, page_title:"Clip", playlist_url, cookies:"a=1"}` → ok. The record's kind is `"hls"`, it has the headers `Cookie` and `Referer`, and `page_title` is "Clip".
  - `api.rs` `suggest_lists_variants_for_master`: a master with 2 variants gives `is_stream`, 2 variants with the highest first, and a label containing `Mbit/s`.
  - `api.rs` `suggest_plain_file_has_no_variants`.
  - `model.test.ts` `progressPct`: `segments {done: 3, total: 12}` → 25; bytes 50 of 200 → 25; no size → 0; the result never goes past 100.
- [ ] **Step 2: Run.** Expected: failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the Rust gates and the app gates. Expected: pass.
- [ ] **Step 5: Commit**
  - Message: `feat(app): add HLS streams from the browser and the Add dialog`

---

### Task 6: Extension: stream detection, popup list, in-page button

**Files:**
- Create: `extension/src/lib/streams.ts`, `extension/src/lib/streams.test.ts`
- Modify: `extension/manifests/chromium.json` and `firefox.json` (add `"webRequest"` to permissions), `extension/src/background.ts`, `extension/src/content.ts`, `extension/src/popup.ts`, `extension/src/popup.html`, `extension/src/popup.css`, `extension/src/lib/protocol.ts` (`AddStreamPayload`)

**Interfaces:**
```ts
export type StreamKind = "hls" | "dash";
export type Stream = { url: string; kind: StreamKind; label: string; supported: boolean };
export function classify(url: string, contentType: string | undefined): StreamKind | null;
//  hls: path ends .m3u8 (ignore query) or content-type application/vnd.apple.mpegurl | application/x-mpegurl | audio/mpegurl (case-insensitive, params ignored)
//  dash: path ends .mpd or content-type application/dash+xml
export function streamLabel(url: string): string;      // "<last path segment> · <host>"
export class StreamStore {
  add(tabId: number, url: string, kind: StreamKind): boolean;  // true if new; dedupe by URL without its fragment
  list(tabId: number): Stream[];                               // insertion order; dash => supported: false
  clear(tabId: number): void;
  count(tabId: number): number;                                // supported (hls) only — used for the badge
}
export type AddStreamPayload = { page_url: string; page_title?: string; playlist_url: string; headers: Record<string, string>; cookies?: string };
export function toStreamPayload(s: Stream, pageUrl: string, pageTitle: string | undefined, cookies: Cookie[], userAgent: string): AddStreamPayload;
//  headers: {"User-Agent": ua, "Referer": pageUrl}
```
**Wiring:**
- **background:**
  - `webRequest.onHeadersReceived.addListener(cb, {urls: ["<all_urls>"], types: ["xmlhttprequest", "media", "other"]}, ["responseHeaders"])`. When `tabId >= 0` and `classify(...)` is non-null, call `store.add`. If the stream is new, update the badge text to `count` (empty when 0) for that tab with the accent background `#6366f1`, and send `tabs.sendMessage(tabId, {type: "odm-streams", streams: store.list(tabId)})`, swallowing errors.
  - `webNavigation` is not used. `tabs.onUpdated` with `changeInfo.status === "loading"` and a `changeInfo.url` → `store.clear(tabId)`, badge cleared. `tabs.onRemoved` → clear.
  - Messages:
    - `{type: "odm-list-streams", tabId}` → the list;
    - `{type: "odm-add-stream", url}` from a content script → uses `sender.tab` for `page_url` and `page_title`;
    - the popup sends `{type: "odm-add-stream", url, tabId}`.
    - Both send `ADD_STREAM` with a 15 000 ms timeout and reply `{ok, message}` (the message is `errorText`, or "Sent to ODM.").
  - The MV3 service worker may restart and lose the in-memory store. That is accepted: the badge resets on restart.
- **content:**
  - On `odm-streams` with at least one supported stream, render (once; update the count after that) a fixed bottom-right button in a closed shadow root: "ODM · N video(s)" ("1 video" or "N videos").
  - Clicking it toggles a list of supported streams, each with a "Download" button, plus a "×" dismiss that hides the button for this page load.
  - A click on "Download" sends `odm-add-stream` and shows the reply message under the item.
  - Never show anything when there are no streams.
  - Styles live inline in the shadow root, using the dark tokens.
- **popup:** a "Videos on this page" section, shown only when the active tab has streams.
  - Each HLS item has a label and a "Download" button.
  - DASH items show "(DASH, not supported yet)" without a button.
  - The reply message is shown inline.
- No stream is ever sent without a click.

- [ ] **Step 1: Write the failing tests** in `streams.test.ts`:
  - `classify`:
    - `.../index.m3u8?token=1` → hls;
    - `application/vnd.apple.mpegurl; charset=utf-8` → hls;
    - `application/x-mpegURL` → hls;
    - `.mpd` → dash;
    - `video/mp4` → null.
  - `stream_store_dedupes_and_clears` (Review Focus 5):
    - adding the same URL twice (and once with a `#frag`) returns true, then false, then false;
    - a second tab is independent;
    - `count` ignores dash;
    - `clear` empties only that tab.
  - `streamLabel("https://cdn.example.com/a/b/index.m3u8?x=1")` → `"index.m3u8 · cdn.example.com"`.
  - `toStreamPayload` sets Referer = the page URL and `cookies` as a header string.
- [ ] **Step 2: Run** `pnpm test`. Expected: failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the extension gates. Expected: pass. The built manifests include `webRequest`.
- [ ] **Step 5: Commit**
  - Message: `feat(extension): detect HLS/DASH streams; popup list and in-page button`

---

### Task 7: Verification and README

- [ ] **Step 1: Controller check.** With the debug app, `odm-host` and the HLS server from a quick Rust test, or `python -m http.server` over a folder made with the Task 2 FFmpeg command:
  - send `ADD_STREAM` through the host;
  - the download goes Downloading → Merging → Completed;
  - the `.mp4` plays according to `ffprobe`.
  - Record the result in `docs/superpowers/plans/2026-10-03-odm-plan-4-smoke.md`.
- [ ] **Step 2: Manual checklist** in the smoke doc:
  1. Open a public HLS test page, for example a player loading `https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8`. The badge shows a count and the in-page button appears.
  2. Popup → Download. ODM shows the stream, it merges, and the mp4 plays.
  3. In the Add dialog, paste the m3u8 URL. "Quality" lists the variants.
  4. Pause in the middle of the segments, quit ODM, restart, resume. It finishes and plays.
  5. A DASH page shows "(DASH, not supported yet)".
  6. With FFmpeg's path set wrong in Settings and FFmpeg not on PATH, the download fails with "FFmpeg not found — set its path in Settings."
- [ ] **Step 3: README:** update "Status" (video streams are done) and add a "Video streams" section covering detection, the FFmpeg requirement and setting, AES-128 support, and what is not supported (live, SAMPLE-AES, DASH, byte-range playlists).
- [ ] **Step 4:** Run all gates. Commit with the message `docs: video streams guide and Plan 4 smoke checklist`.
