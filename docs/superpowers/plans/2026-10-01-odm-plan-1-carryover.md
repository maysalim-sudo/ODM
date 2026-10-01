# ODM Plan 1: carry-over for Plans 2 and 3

Plan 1 (workspace, core, store, engine, CLI) is complete on branch `feat/plan-1-engine`: 113 tests pass, fmt and clippy are clean.
This file keeps the decisions and the deferred findings from Plan 1's reviews, so the next plans can pick them up.

## Decisions made during execution (rulings)

- Model tiers — Tasks 0-3 implementer haiku; Tasks 4-15 implementer sonnet (plan code is complete but was never compiled, so fixes need judgment); reviewers sonnet, except Tasks 7/10/12/13 (concurrency/correctness core) reviewed on opus; final review opus — cheapest tier that can debug uncompiled Rust — if wrong, extra fix rounds.
- Work on branch feat/plan-1-engine in place (no separate worktree) — repo has only docs commits and no other work in flight — cost if wrong: none beyond a branch switch.
- Commit Cargo.lock (workspace ships binaries) as part of Task 4's commit — reproducible builds — cost if wrong: none.
- Task 4 Important (plan-mandated) "unknown persisted state silently maps to Failed" (repo.rs:845) — code stands: failing the row conversion would make list_downloads error and brick the whole list over one bad row; the only source of unknown states is a future-version DB, which Minor #1 (reject newer user_version) addresses better — cost if wrong: a corrupted row shows as Failed without an error message.
- Cargo.lock drift left uncommitted by Tasks 5-7 (new engine deps) gets committed in Task 8's commit — lockfile belongs in git — cost if wrong: none.
- Task 8 Important (plan-mandated) "max_concurrent() over-counts: in_flight never decremented when Content-Length is set" (lib.rs:251-263) — FIX IT with a Drop guard in the stream state and peak = fetch_add(1)+1 — later tests assert max_concurrent() (Task 12 >= 2) and an over-count makes that assertion meaningless; the brief's code is the plan's argument, not the spec — cost if wrong: one small extra fix round.
- Remove reqwest's "gzip" feature from the workspace dependency (carried into Task 12) — a download manager must save the server's bytes exactly; transparent decompression breaks size checks and Range offsets (reviewer T10 minor #5) — cost if wrong: servers that only serve gzip-encoded bodies of uncompressed files save compressed bytes (rare; browsers' own downloads behave the same).
- download::header_map must drop Range, If-Range, Content-Length, Host, Connection, Transfer-Encoding and Accept-Encoding from stored/browser headers (carried into Task 12) — reqwest .header() appends, so a captured Range would be sent twice and corrupt chunks (reviewer T10 ⚠️) — cost if wrong: none; those headers are never meaningful to replay.
- Task 12 must treat Ok from an open-ended chunk worker as complete even when end stays None (empty unknown-size body) — reviewer T10 minor #1; plan's transfer() already relies on JoinSet results not pending(), confirm in Task 12 review — cost if wrong: a 0-byte unknown-size download never finishes.
- Task 12 Important #1 (plan-mandated) "missing/replaced part file → resume reports Completed with zero-filled regions" — FIX: before transfer, if any stored chunk has written>0 and the part file is missing or its length is wrong (size known: len != size; unknown: len < total written), discard progress (replace_chunks(vec![]) → fresh probe/plan) and warn; in the restart branch call replace_chunks BEFORE remove_file — spec §1 success criterion 3 (byte-identical after crash/resume) is binding — cost if wrong: a paused download whose part file was deleted restarts from zero (correct behaviour anyway).
- Task 12 Important #2 (plan-mandated) "non-resumable downloads get exactly one recovery" — FIX: replace the single `restarted` flag with a restart budget of config.max_attempts, sleeping backoff_delay(restarts) (cancellable) between restarts; RangeIgnored/FileChanged still reset chunks as now — spec §5.5 requires transient errors to be retried; bounded by max_attempts so no infinite loop — cost if wrong: a truly changing server file is re-fetched up to 5 times before failing.
- Task 14 Important (plan-mandated) "deadlock test never proves heavy concurrency (8 conns/download, overlap unasserted)" — FIX: throttle the server per connection (bytes_per_sec ~1 MiB) so downloads overlap, and assert TestServer::max_concurrent() > 32 (more simultaneous connections than the old fixed pool had); keep 32 downloads, max_concurrent 10, max_connections 32 — a regression test that can't observe the load it names doesn't guard anything — cost if wrong: ~5 s longer test.
- Final #1 (plan-mandated) cookies replayed to cross-origin final_url + pinned expiring signed URL — FIX: workers request rec.url (reqwest re-follows redirects per request and strips Cookie/Authorization cross-origin); final_url for display only — cost if wrong: one extra redirect hop per connection.
- Final #2 blocking write_at + NTFS zero-fill stalls the runtime — FIX: positioned writes via spawn_blocking; mark part file sparse on Windows (FSCTL_SET_SPARSE) before set_len — cost if wrong: small per-write overhead.
- Final #3 200-to-Range with unchanged validator misreported as FileChanged — FIX: compare response validator with ctx.validator; unchanged/absent → RangeIgnored, changed → FileChanged (spec §5.2.4) — cost if wrong: none.
- Final #4 (plan-mandated) ChunkView lacks idx/state required by spec §5.7/§9; JSON unpinned — FIX: add idx + state (pending|active|retrying|done|failed) and a serde_json snapshot test of every EngineEvent variant — Plan 2 serializes events directly — cost if wrong: none.
- Final #5 panic in run_download wedges the download in `active` forever — FIX: inner task + JoinError → Outcome::Failed(Other, "internal error") so Finished is always sent — cost if wrong: none.
- add a doc comment on ManagerHandle::subscribe(): Lagged ⇒ resync from store (final recommendation 2; cheap).
- Final Minors #1-#9 and the triage's "fix before Plan 2/3" items stay deferred and are carried into Plans 2/3 (set_config, read-only store view, atomic add+headers, C-D parsing, reject newer DB, set_rate waking sleepers, Lagged resync in subscribers, checksum-retry discard, rename retry under AV locks, userinfo in log_url, header-name normalization, EngineConfig validation, 416-without-Content-Range, crash between rename and Completed) — they live in APIs/paths those plans touch — cost if wrong: they surface when those plans start.
- continue the SAME fix wave with a fresh sonnet fixer that takes over the uncommitted work (not a second wave: the first never completed) - opus is rate-limited - cost if wrong: a weaker model finishes items 4-6; the scoped re-review still gates it.

## Deferred findings (not fixed in Plan 1)

The final review sorted these. Items it marked "fix before Plan 2/3" are listed in the last ruling above; address them at the start of the plan that touches that code.

- Task 0-3: minor (deferred): reserved-name check misses "CON .txt" (stem not trimmed) — filename.rs:345
- Task 0-3: minor (deferred): unique_name can exceed 200 chars / name.tar.gz → name.tar (1).gz — filename.rs:395-407
- Task 0-3: minor (deferred): Content-Disposition split(';') breaks quoted ';' ; filename* charset ignored — filename.rs:380
- Task 0-3: minor (deferred): `.ts` classed as Video (TypeScript collision) — category.rs:150 (plan-mandated)
- Task 0-3: minor (deferred): thiserror dep unused in odm-core (plan-mandated)
- Task 4: minor (deferred): migrate() doesn't reject user_version > MIGRATIONS.len() (newer DB) — migrations.rs:735
- Task 4: minor (deferred): no CHECK(state IN ...) on downloads/hls_segments — migrations.rs:678,718
- Task 4: minor (deferred): writes to missing rows succeed silently (affected-row counts ignored) — repo.rs:904-990
- Task 4: minor (deferred): panic in a store job kills the owner thread permanently — lib.rs:585
- Task 4: minor (deferred): WAL pragma result + create_dir_all error ignored — lib.rs:566,577
- Task 4: minor (deferred): unchecked u64→i64 casts; test gaps (FK/WAL/COALESCE/header cascade/rollback)
- Task 5-6: minor (deferred): backoff jitter applied after cap → up to 72 s (plan-mandated) — retry.rs:128
- Task 5-6: minor (deferred): classify_io raw codes 112/39 not cfg(windows)-gated (ENOTEMPTY/EHOSTDOWN on unix) — retry.rs:155
- Task 5-6: minor (deferred): is_request/is_decode → Connection (transient) retries malformed requests — retry.rs:165
- Task 5-6: minor (deferred): limiter: set_rate doesn't wake sleepers; cancelled acquire keeps debt; tiny-limit test has no lower bound
- Task 7: minor (deferred): open-ended chunk never reports is_done → download task must use worker results, not pending().is_empty() (plan already does) — chunks.rs:42
- Task 7: minor (deferred): undocumented caller contracts (split_largest locks every chunk, non-reentrant; worker must stop on allowed==0) — chunks.rs:58,85
- Task 7: minor (deferred): thin tests (repeated splits, pending/progress, n==7 assertion)
- Task 8: minor (deferred): len-0 /file without Range advertises Content-Length: 1 (plan-mandated) — lib.rs:220,243
- Task 8: minor (deferred): drop_after >= body len still emits the error after the full body — lib.rs:223-228
- Task 8: minor (deferred): smoke test covers only range/416/no_ranges/drop (plan-mandated)
- Task 8: minor (deferred): sequential max_concurrent test assumes hyper drops body before next request (theoretical flake)
- Task 9-11: minor (deferred): 416 without/unparseable Content-Range → size 0 (plan-mandated) — probe.rs:203
- Task 9-11: minor (deferred): no test pins EngineEvent JSON shape (UI will depend on it) — events.rs
- Task 9-11: minor (deferred): save_dir_for joins unsanitized category folder values — config.rs:71
- Task 9-11: minor (deferred): probe tests thin (validator fallback, bytes 0-0/*, filename*, charset)
- Task 10: minor (deferred): early-EOF branch, shrinking-end race and local_limit path untested — worker.rs:198
- Task 10: minor (deferred): limiter charged for bytes discarded past end — worker.rs:173
- Task 10: minor (deferred): blocking write_at on runtime thread (fine for 16 KiB pieces; revisit if stalls) — worker.rs:180
- Task 12: minor (deferred): unknown-size part never truncated to total_written; size recorded from metadata — download.rs:169,446
- Task 12: minor (deferred): probe retry ignores Retry-After — download.rs:215
- Task 12: minor (deferred): plan_chunks/first spawn loop don't clamp to max_conn on resume — download.rs:148,266
- Task 12: minor (deferred): FileChanged re-probe via COALESCE can keep stale validator — store repo update_probe
- Task 12: minor (deferred): persistent_transient test likely only exercises probe retry, not chunk budget — tests/download.rs:121
- Task 12: minor (deferred): headers fetched twice; sync fs ops on runtime — download.rs:128,253
- Task 13: minor (deferred): command sent just after shutdown reply returns Ok but is dropped — call rx.close() before replying (plan-mandated) — manager.rs ~diff 229,236
- Task 13: minor (deferred): tasks still running at shutdown deadline stay probing/downloading; same-process restart could double-write (plan-mandated)
- Task 13: minor (deferred): outcome matrix loses info (Pause+Failed→Paused, Shutdown+Failed→Queued, Remove+Completed emits Completed first)
- Task 13: minor (deferred): actor never exits when all handles dropped (holds own tx)
- Task 13: minor (deferred): store errors swallowed in state_of/ids_in/delete_record
- Task 13: minor (deferred): second shutdown() replaces first reply; add() not atomic (insert ok, set_headers fails); scheme check case-sensitive
- Task 13: minor (deferred): test gaps (pause_all/resume_all, set_max_concurrent, priority, remove-active)
- Task 14: minor (deferred): crash test doesn't assert it cut mid-download / kept earlier bytes
- Task 14: minor (deferred): preallocation poll can see a 0-length file between state change and set_len (tiny race)
- Task 14: minor (deferred): deadlock test doesn't shut the manager down; failure message doesn't name the download
- Task 15: minor (deferred): Lagged events ignored → could hang if terminal State event is dropped; re-check store on Lagged (plan-mandated) — lib.rs:116
- Task 15: minor (deferred): early `?` returns skip m.shutdown() (plan-mandated) — lib.rs:92,103
- Task 15: minor (deferred): progress line padding leaves stale chars; no test for Cancelled / -c / --limit
- Final: minor (deferred): spawn_blocking write can complete after the worker future is dropped on error-exit paths (<= one buffer; written unaffected) - worker.rs ~181
