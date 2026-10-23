# Torrent downloads: decisions made during implementation

Plan: `2026-10-23-odm-torrents.md`. Each line: the decision, why, and what it costs if wrong. Copied from the implementation ledger.

## Rulings

- Ruling: work directly on master (no worktree/feature branch) — user's standing ODM convention is a single master branch; nothing pushed without asking — cost if wrong: none, commits are local.
- Task 3: Ruling: TorrentSettings gains `offline: bool` (no DHT/trackers/LSD/UPnP) — local-seeder tests must not reach the internet — cost if wrong: one unused field in production.
- Task 3: Ruling: LibrqbitClient gains inherent `restore()` and `shutdown()` (not on the trait) — fastresume restart test needs an explicit session load/stop; engine calls shutdown on exit (Task 7) — cost if wrong: engine must call it or saved state may restore unpaused (pause_all on create covers that).
- Task 3: Ruling: selected_total/done are summed from file sizes and per-file progress, not librqbit's piece-aligned hns — spec says selected files only (hns gave 100848 for 80000) — cost if wrong: none.
- Task 3: Ruling: set_files and start-on-existing wait_until_initialized first — librqbit refuses file changes while checking — cost if wrong: brief wait.
- Task 3: Ruling: start() on a torrent already in the session removes and re-adds it (data kept, rechecked) when it has new trackers, or when extra_peers are set and this client didn't add it — merged trackers must reach librqbit on start (spec dedupe), and librqbit has no live tracker/peer API — cost if wrong: one local recheck after a duplicate add.
- Task 3: Ruling: StartTorrent.save_dir is the exact output folder (librqbit puts files directly in it); the engine decides save_dir/name for multi-file torrents (Task 7) — cost if wrong: files land one level up.
- Task 5: Ruling: torrent_files gains a `done` column and Store::save_torrent_progress; list/get_with_bytes add SUM(MIN(done,size)) of selected files — the list view's byte count comes from SQL for every kind, and a torrent has no chunks — cost if wrong: one extra column.
- Task 6: Ruling: TorrentRequest drops `stored_torrent`; the engine itself writes .torrent bytes (given or rebuilt from fetched metadata) to `<torrent.state_dir>/files/<hash>.torrent` and switches torrent_meta.source to that path when metadata arrives — one owner for the files, and a confirmed magnet need not refetch metadata from peers — cost if wrong: none.
- Task 6: Ruling: added odm_torrent::with_trackers (rebuilds .torrent announce-list around the raw info bytes) and LibrqbitClient::start uses it for Bytes sources — librqbit takes trackers only from the source, so merged trackers must be baked in — cost if wrong: comment/creation-date keys dropped from the rebuilt file.
- Task 6: Ruling: the torrent client lives in DownloadEnv.torrents (default LibrqbitClient, replaced by start_with_torrents(config, store, client)); start_with_torrents has no clock parameter — run_torrent keeps the (env, id, cancel) runner shape — cost if wrong: none.
- Task 6: Ruling: the limit-window tick and SetSpeedLimit/SetConfig all call apply_torrent_settings with the effective global rate — the spec says torrents follow the global limit — cost if wrong: none.
- Task 6: Ruling: on start the manager refetches metadata for paused magnet rows without metadata (restart_refetches_metadata) — otherwise a restart strands "Fetching file list…" rows — cost if wrong: none.
- Task 6: Ruling: FakeCall::FetchCancelled added so the cancel test can observe cancellation — cost if wrong: none.
- Task 7: Ruling: "Download all" before metadata makes run_torrent fetch metadata first inside its queue slot (state Probing) and then start with the full file list — keeps the output folder rule (one file → save folder; several → save/<name>) stable — cost if wrong: a second peer lookup for that magnet.
- Task 7: Ruling: on finish, every selected file's stored `done` is set to its size — the list byte count must show complete regardless of client per-file figures — cost if wrong: none.
- Task 7: Ruling: set_torrent_files emits EngineEvent::TorrentMetadata so the UI reloads the file list; appcore bridge maps it to Upsert + UiEvent::TorrentMetadata, and UiEvent::Focus was added (Task 8/9 scope, needed to compile) — cost if wrong: none.
- Task 7: Ruling: removing/cancelling deletes files via the client and also deletes each listed file under the output folder — a completed torrent is no longer in the session, so the client can't delete it — cost if wrong: a same-named folder the user created by hand inside the save folder could be removed when it equals the torrent's folder name.
- Task 7: Ruling: ManagerHandle keeps an Arc of the torrent client for pieces() (read-only, no actor round trip) — cost if wrong: none.
- Task 8: Ruling: appcore::torrent::resolve returns a TorrentSource (not a TorrentRequest) and takes (input, http, timeout) — Task 6 made the engine the owner of stored .torrent files — cost if wrong: none.
- Task 8: Ruling: api::add_torrent(m, pending, http, input) — http client passed in (IPC builds it from settings via build_client) — cost if wrong: none.
- Task 8: Ruling: an already-listed torrent added over IPC only brings the window up (no Focus via IPC; IpcContext has no focus callback) — the Tauri add command emits Focus in Task 9 — cost if wrong: a browser re-add doesn't select the row.
- Task 8: Ruling: PieceMapDto carries `have_hex` instead of `have_b64` — no base64 crate in the workspace — cost if wrong: 2x payload for the piece map (small).
- Task 8: Ruling: DownloadDto.has_meta = size known (torrent rows only) — replace_torrent_files is the only writer of a torrent's size — cost if wrong: none.
- Task 8: Ruling: TorrentFileDto.done comes from stored progress (saved every ≥5 s while running) — no live per-file stats through the detail call — cost if wrong: per-file bars lag up to ~5 s.
- Task 8: Ruling: TorrentSettings::default().state_dir = %TEMP%/odm-torrents (was empty → working directory); appcore test fixture uses a temp state dir and offline mode — a stray files/ dir appeared in crates/odm-appcore during the first run and was removed — cost if wrong: none.
- Task 8: Ruling: ADD_TORRENT added to starts_app so a magnet click can start ODM — spec intake — cost if wrong: none.
- Task 8: Ruling: torrent_port below 1024 (non-zero) is reset to 0 → re-chosen; torrent_max_peers clamps 10..1000 — cost if wrong: none.
- Task 9: Ruling: commands::open_torrent (shared by add_torrent, the single-instance callback and the startup argument) emits Focus for an existing torrent and ConfirmDownload for a new one — the UI loads pending confirms on start, so a startup add is not lost before the window listens — cost if wrong: none.
- Task 9: Ruling: settings.ensure_torrent_port() runs right after Settings::load at startup and saves when it picked a port — spec: chosen on first run and kept — cost if wrong: none.
- Task 9: Ruling: TS types/api added here (types.ts, api.ts) plus the two new UiEvent cases in model.ts's exhaustive switch, and the settings test fixture gained the four torrent fields — needed for svelte-check — cost if wrong: none.
- Task 10: Ruling: piece map is its own PieceMap.svelte (cells from the hex bitfield via fileTree.pieceCells) instead of changing ChunkMap — ChunkMap's geometry is byte ranges of HTTP parts — cost if wrong: none.
- Task 10: Ruling: the confirm dialog shows no free-space figure — there is no free-space command in the app; adding one is beyond this plan — cost if wrong: user can't see free space before downloading a big torrent.
- Task 10: Ruling: Torrents get their own settings section ("Torrents", after Network) rather than a card inside Network — four rows; settingsPage section-order test updated — cost if wrong: none.
- Task 10: Ruling: save_settings always runs ensure_torrent_port (Restore defaults brings back port 0) — test saving_port_zero_picks_one; api.rs proxy test fixture got a fixed port — cost if wrong: none.
- Task 10: Ruling: "torrent" added to CATEGORIES (sidebar "Torrents") but hidden from the Add dialog's Type list for HTTP downloads — cost if wrong: none.
- Task 10: Ruling: manual look (Step 5) deferred to the end / smoke checklist — the user's installed ODM is running and a debug build would hand off to it through the single-instance plugin; closing the user's app is not ours to do — cost if wrong: UI issues found later.
- Task 11: Ruling: passive torrent captures (magnet clicks, .torrent downloads; source "capture") follow "Take over downloads from the browser" — appcore replies `skipped` when it is off and the browser keeps the link/download; the context menu (source "menu") is always taken — test capture_off_leaves_torrent_links_to_the_browser — cost if wrong: none.
- Task 11: Ruling: magnet clicks are caught in the content script (capture-phase click on a[href^=magnet:], skipped with Alt/Ctrl/Shift/Meta held); without an ok reply the page navigates to the magnet so the OS handler still runs — cost if wrong: a slow ODM start (15 s send timeout) delays the fallback.
- Task 11: Ruling: toTorrentPayload takes a 6th `source` argument (capture | menu) — the app needs it for the capture rule above — cost if wrong: none.
- Task 12: Ruling: one Yes/No question ("Open magnet links and .torrent files with ODM?", default No) in NSIS_HOOK_POSTINSTALL instead of two installer checkboxes — Tauri's NSIS hooks cannot add installer pages; silent, passive (/P) and update (/UPDATE) runs never ask and keep earlier choices — cost if wrong: the user cannot pick magnet without .torrent at install (both stay changeable in Windows Default apps).
- Task 12: Ruling: open command written inline in the macros (an early !define of ODM_OPEN_CMD sat before MAINBINARYNAME was defined) — fixed in a follow-up commit; makensis compile re-verified — cost if wrong: none.
- Final: Ruling: C2 — torrents with a path part Windows cannot use (drive-style `c:x` which escapes the output folder, `<>:"|?*`, control chars, trailing dot/space, CON/PRN/AUX/NUL/COM0-9/LPT0-9 with any extension) are refused with "This torrent has a file name Windows can't use: <path>" — librqbit has no output-path hook to rename them, and writing them is a path-escape risk — cost if wrong: such torrents can't be downloaded at all (spec/Review Focus 5 said "must succeed"; reported to the user).
- Final: Ruling: spec deviation — no free-space figure in the confirm dialog (reviewer: spec line 124 requires it) stands for now; needs a new command; reported to the user — cost if wrong: user can't see free space before a big torrent.
- Final: Ruling: spec deviation — an IPC re-add of an existing torrent brings the window up but doesn't select the row (spec line 118) stands; reported — cost if wrong: minor.

## Deferred minors (from the final review)

- LibrqbitClient::shutdown is never called at app exit; exit relies on per-torrent pause within the 3 s grace (ruling 25's premise).
- set_torrent_files accepts unknown indices (e.g. [999]) and can leave zero files selected.
- DetailPanel keeps a checkbox visually unticked after "Keep at least one file selected." is refused.
- extension .torrent capture uses the 5 s capture timeout; the app may take up to 15 s to fetch.
- uninstall deletes the whole HKCU\Software\Classes\.torrent key (other apps' OpenWithProgids) and never restores the previous default.
- command-line/single-instance open_torrent errors are only logged; a relative path from a second instance resolves against the first instance's cwd.
- "Download all" before metadata leaves the background fetcher running alongside the in-slot fetch.
- run_torrent waits forever when stats() returns None (handle gone).
- the .torrent copy is written before insert_download and is orphaned if the insert fails.
- with_trackers also drops `nodes` and `url-list` from a rebuilt .torrent.

## Final review fixes

- C1 (cancel/remove-with-files used remove_dir_all on save/<name>, deleting unrelated user files) — torrent::delete_files deletes only listed files under the output folder (paths must be plain relative parts) and removes emptied folders with remove_dir — cancel_deletes_only_the_torrents_files RED→GREEN.
- C2 — check_names in parse_bytes and in LibrqbitClient::fetch_metadata — names_windows_cannot_use_are_refused RED→GREEN (hand-written bencode); misleading reserved_names_download replaced by spaces_and_accents_download; overruled the Task 3 note "no ODM sanitizing needed".
- I1 (remove before the lazy session started left the torrent in the saved session; a restored torrent ignored a new save folder) — remove() starts the session when needed; needs_readd re-adds when output_folder differs — remove_before_session_start_forgets_it and restored_torrent_uses_a_new_folder RED→GREEN.
- I4 (a settings change shut the session down under a running metadata fetch; a busy session never restarted) — in-flight fetches count as busy; restart_pending is applied when the client next goes idle (after a fetch, pause or remove) — settings_change_keeps_a_metadata_fetch_alive RED→GREEN.
- I2 (a Cancelled torrent blocked every later add of the same torrent) — add over a Cancelled row deletes it and adds fresh; Completed/other rows still dedupe — cancelled_torrent_can_be_added_again RED→GREEN.
- I3 (a .torrent for a magnet still fetching its file list was dropped) — the existing row's fetch stops and the .torrent's metadata is stored — torrent_file_completes_a_waiting_magnet RED→GREEN.
- I6 (metadata fetch failures only logged; row spun forever) — a non-cancelled fetch error, or fetched names Windows cannot use, fails the waiting row with the message; Resume retries in the queue slot — metadata_failure_is_shown and fetched_names_windows_cannot_use_fail_the_row RED→GREEN.
- I5 (torrents wrote over and could later delete files already at their target) — a torrent that never downloaded (started_at unset) and whose target files exist goes to "<name> (n)" (multi-file) or a "<stem> (n)" folder (single file); resumed torrents keep their folder — existing_folder_is_never_written_over and existing_single_file_is_never_written_over RED→GREEN, a_resumed_torrent_keeps_its_folder guards it. Suite: cargo test --workspace 52/52 suites, clippy clean, app 203/203, extension 115/115.
