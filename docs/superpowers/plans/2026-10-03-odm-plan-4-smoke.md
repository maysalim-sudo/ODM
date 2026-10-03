# Plan 4 smoke check: HLS video streams

## Controller-run checks (2026-10-03)

**Setup:**
- The debug app (`pnpm tauri build --debug --no-bundle`) was started with `--background`, a throwaway data dir, a throwaway download dir and a test pipe.
- The fixture was a real 6-second HLS stream made with FFmpeg 9.0 (six 1-second TS segments), served over local HTTP.

**ADD_STREAM through `odm-host`:**

```
{"id":"s1","ok":true,"payload":{"download_id":"314027b26a9840b1b61cde635249fc77"}}
```

- The row went to Completed: `Plan 4 check.mp4`, 111.6 KB. The page title "Plan 4: check?" was sanitised into the file name.
- ffprobe duration: 6.09 s.
- The Mark of the Web was present: `ZoneId=3`, `ReferrerUrl=http://127.0.0.1:8799/`, `HostUrl=http://127.0.0.1:8799/index.m3u8`.
- The `.odm-hls-*` temp dir was removed.

**An unreachable playlist** (port taken by another program) failed cleanly with "The server could not find this file (error 404)."

**Add dialog suggestion for a master playlist:** `is_stream: true` with the variants sorted highest first. The labels were `1280×720 · 2.5 Mbit/s` and `320×240 · 800 kbit/s`.

**Automated tests** (`crates/odm-engine/tests/hls.rs`, 12 tests, real FFmpeg on this machine):
- TS merge and fMP4 merge (duration 3.5–4.5 s for a 4 s clip);
- AES-128 encrypted merge;
- pause/resume skipping finished segments;
- live and SAMPLE-AES failing fast;
- missing FFmpeg message;
- corrupt segments giving FFmpeg's stderr;
- cancel removing the temp dir.

## Manual checklist (person at the desktop)

- [ ] 1. Open a page that plays a public HLS test stream (for example `https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8` in an HLS player page). The badge shows a count, and the in-page "ODM" button appears.
- [ ] 2. Popup → Download. ODM shows the stream, merges it, and the mp4 plays.
- [ ] 3. In the Add dialog, paste the m3u8 URL. "Quality" lists the variants.
- [ ] 4. Pause in the middle of the segments, quit ODM, restart, resume. It finishes and plays.
- [ ] 5. A DASH page shows "(DASH, not supported yet)".
- [ ] 6. With the FFmpeg path set wrong in Settings and FFmpeg not on PATH, the download fails with "FFmpeg not found — set its path in Settings."
