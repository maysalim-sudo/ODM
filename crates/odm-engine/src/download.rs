use md5::Md5;
use odm_core::{filename, DownloadState, ErrorKind, OdmError};
use odm_store::{ChunkRecord, DownloadRecord, ProbeMeta, Store, StoreError};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
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
use crate::events::{ChunkViewState, EngineEvent, ProgressSnapshot};
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

pub fn part_path(rec: &DownloadRecord) -> PathBuf {
    let short = &rec.id[..rec.id.len().min(8)];
    Path::new(&rec.save_dir).join(format!("{}.{short}.odm-part", rec.filename))
}

/// Headers that must not be replayed from a captured browser request: the engine
/// sets its own Range, and reqwest computes the rest.
const REPLAY_UNSAFE: [&str; 7] = [
    "range",
    "if-range",
    "content-length",
    "host",
    "connection",
    "transfer-encoding",
    "accept-encoding",
];

pub fn header_map(h: &[(String, String)]) -> HeaderMap {
    let mut m = HeaderMap::new();
    for (k, v) in h {
        if REPLAY_UNSAFE.iter().any(|u| k.eq_ignore_ascii_case(u)) {
            continue;
        }
        if let (Ok(k), Ok(v)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
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
    rec.etag
        .clone()
        .filter(|e| !e.starts_with("W/"))
        .or_else(|| rec.last_modified.clone())
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

async fn run_inner(
    env: &DownloadEnv,
    id: &str,
    cancel: &CancellationToken,
) -> Result<PathBuf, OdmError> {
    let mut restarts: u32 = 0;
    loop {
        let mut rec = load(env, id).await?;
        let headers = header_map(&env.store.get_headers(id).await.map_err(store_err)?);
        let mut chunks = env.store.get_chunks(id).await.map_err(store_err)?;
        let mut require_partial = chunks.len() > 1;

        let total_written: u64 = chunks.iter().map(|c| c.written).sum();
        if total_written > 0 {
            let len = std::fs::metadata(part_path(&rec)).ok().map(|m| m.len());
            let bad = match (len, rec.size) {
                (None, _) => true,
                (Some(l), Some(size)) => l != size,
                (Some(l), None) => l < total_written,
            };
            if bad {
                tracing::warn!(%id, "part file missing or wrong size; discarding stored progress");
                env.store
                    .replace_chunks(id, Vec::new())
                    .await
                    .map_err(store_err)?;
                continue;
            }
        }

        if chunks.is_empty() {
            transition(env, id, DownloadState::Probing, None).await;
            let p = probe_with_retry(env, &rec.url, &headers, cancel).await?;
            env.store
                .update_probe(
                    id,
                    ProbeMeta {
                        final_url: Some(p.final_url.clone()),
                        size: p.size,
                        mime: p.mime.clone(),
                        etag: p.etag.clone(),
                        last_modified: p.last_modified.clone(),
                        filename: p.filename.clone(),
                    },
                )
                .await
                .map_err(store_err)?;
            rec = load(env, id).await?;
            if rec.auto_place {
                let detected = odm_core::category::detect(&rec.filename, rec.mime.as_deref());
                if detected.as_str() != rec.category {
                    let dir = env.config.save_dir_for(detected.as_str());
                    env.store
                        .set_placement(
                            id,
                            detected.as_str().to_string(),
                            dir.to_string_lossy().into_owned(),
                        )
                        .await
                        .map_err(store_err)?;
                    rec = load(env, id).await?;
                }
            }
            require_partial = p.supports_range && p.size.is_some();
            chunks = plan_chunks(p.size, require_partial, rec.max_connections);
            env.store
                .replace_chunks(id, chunks.clone())
                .await
                .map_err(store_err)?;
        }

        transition(env, id, DownloadState::Downloading, None).await;
        std::fs::create_dir_all(&rec.save_dir).map_err(io_err)?;
        let part = part_path(&rec);
        tracing::info!(%id, url = %odm_core::redact_url(&rec.url), part = %part.display(), "downloading");

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
                preallocate(&file, size).map_err(io_err)?;
            }
        }

        match transfer(env, &rec, Arc::new(file), &chunks, require_partial, cancel).await {
            Ok(()) => return finish(env, &rec, &part).await,
            Err(e)
                if restarts < env.config.max_attempts
                    && !cancel.is_cancelled()
                    && matches!(e.kind, ErrorKind::RangeIgnored | ErrorKind::FileChanged) =>
            {
                tracing::warn!(%id, "restarting from zero: {e}");
                restarts += 1;
                tokio::select! {
                    _ = cancel.cancelled() => return Err(cancelled()),
                    _ = tokio::time::sleep(backoff_delay(restarts, None)) => {}
                }
                let next = if e.kind == ErrorKind::RangeIgnored {
                    vec![ChunkRecord {
                        idx: 0,
                        start: 0,
                        end: rec.size.map(|s| s - 1),
                        written: 0,
                    }]
                } else {
                    Vec::new() // forces a fresh probe
                };
                env.store
                    .replace_chunks(id, next)
                    .await
                    .map_err(store_err)?;
                let _ = std::fs::remove_file(&part);
            }
            Err(e) => return Err(e),
        }
    }
}

/// Sizes the part file to `size` before any chunk writes.
///
/// On Windows the file is first marked sparse: otherwise NTFS zero-fills up to
/// the valid-data length synchronously on the first write at a high offset
/// (seconds for multi-GB files). Failure to set sparse is not fatal.
pub(crate) fn preallocate(file: &File, size: u64) -> std::io::Result<()> {
    #[cfg(windows)]
    if let Err(e) = set_sparse(file) {
        tracing::debug!("could not mark part file sparse: {e}");
    }
    file.set_len(size)
}

#[cfg(windows)]
fn set_sparse(file: &File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;
    use windows_sys::Win32::System::IO::DeviceIoControl;
    let mut returned: u32 = 0;
    // SAFETY: the handle is owned by `file` and valid for the call; a null
    // input buffer means "set sparse"; no output buffer; synchronous I/O.
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
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

/// Pause before a parked chunk takes the place of a finished connection.
const UNPARK_DELAY: Duration = Duration::from_millis(250);

fn set_chunk_state(chunk: &ChunkRef, state: ChunkViewState) {
    chunk.lock().unwrap().state = state;
}

fn spawn_worker(set: &mut JoinSet<Joined>, ctx: &ChunkCtx, chunk: ChunkRef, delay: Duration) {
    let ctx = ctx.clone();
    set_chunk_state(
        &chunk,
        if delay.is_zero() {
            ChunkViewState::Active
        } else {
            ChunkViewState::Retrying
        },
    );
    set.spawn(async move {
        if !delay.is_zero() {
            tokio::select! {
                _ = ctx.cancel.cancelled() => return (chunk, Err(cancelled().into())),
                _ = tokio::time::sleep(delay) => {}
            }
            set_chunk_state(&chunk, ChunkViewState::Active);
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
        // Always the ORIGINAL url: reqwest re-follows redirects on every request and
        // strips Cookie/Authorization on cross-origin hops. `final_url` is display
        // metadata only (and is often a short-lived signed CDN URL).
        url: rec.url.clone(),
        headers: header_map(&env.store.get_headers(id).await.map_err(store_err)?),
        validator: validator_of(rec),
        file,
        global_limit: env.limiter.clone(),
        local_limit: rec.speed_limit.map(|b| Arc::new(SpeedLimiter::new(b))),
        cancel: work.clone(),
        require_partial,
        session_bytes: session.clone(),
    };
    let mut max_conn = rec.max_connections.clamp(1, 32) as usize;

    let mut set: JoinSet<Joined> = JoinSet::new();
    let mut layout_dirty = false;
    for c in table.pending() {
        spawn_worker(&mut set, &ctx, c, Duration::ZERO);
    }
    if require_partial {
        while set.len() < max_conn {
            let Some(c) = table.split_largest() else {
                break;
            };
            layout_dirty = true;
            spawn_worker(&mut set, &ctx, c, Duration::ZERO);
        }
    }

    // idx -> (failures since last progress, chunk written at its last failure,
    //         whole-download written at its last failure)
    let mut attempts: HashMap<u32, (u32, u64, u64)> = HashMap::new();
    // Chunks the server refused (429/503) while other connections were working.
    // They wait, in order, for a free connection instead of burning retries.
    let mut parked: VecDeque<ChunkRef> = VecDeque::new();
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
                        set_chunk_state(&chunk, ChunkViewState::Done);
                        if !work.is_cancelled() && set.len() < max_conn {
                            if let Some(c) = parked.pop_front() {
                                // Let the server release the finished connection first.
                                spawn_worker(&mut set, &ctx, c, UNPARK_DELAY);
                            } else if require_partial {
                                if let Some(c) = table.split_largest() {
                                    layout_dirty = true;
                                    spawn_worker(&mut set, &ctx, c, Duration::ZERO);
                                }
                            }
                        }
                    }
                    Err(f) if f.error.kind == ErrorKind::Cancelled => {
                        set_chunk_state(&chunk, ChunkViewState::Pending);
                    }
                    Err(f)
                        if matches!(f.error.kind, ErrorKind::HttpTransient(429 | 503))
                            && fatal.is_none()
                            && !work.is_cancelled()
                            && !set.is_empty() =>
                    {
                        // The server is refusing this connection while others work:
                        // it limits parallel connections. Wait for a free one and
                        // never ask for more than are working now.
                        tracing::debug!(%id, "server refused a connection ({}); parking chunk", f.error);
                        set_chunk_state(&chunk, ChunkViewState::Pending);
                        parked.push_back(chunk);
                        max_conn = max_conn.min(set.len().max(1));
                    }
                    Err(f) if f.error.kind.is_transient() && fatal.is_none() => {
                        let (idx, written) = { let c = chunk.lock().unwrap(); (c.idx, c.written) };
                        let total = table.total_written();
                        let entry = attempts.entry(idx).or_insert((0, written, total));
                        if written > entry.1 || total > entry.2 {
                            entry.0 = 0; // the chunk or the whole download progressed
                        }
                        entry.1 = written;
                        entry.2 = total;
                        entry.0 += 1;
                        if entry.0 < cfg.max_attempts {
                            tracing::debug!(%id, idx, attempt = entry.0, "retrying chunk: {}", f.error);
                            spawn_worker(&mut set, &ctx, chunk, backoff_delay(entry.0, f.retry_after));
                        } else {
                            set_chunk_state(&chunk, ChunkViewState::Failed);
                            fatal = Some(f.error);
                            work.cancel();
                        }
                    }
                    Err(f) => {
                        if fatal.is_none() {
                            set_chunk_state(&chunk, ChunkViewState::Failed);
                            fatal = Some(f.error);
                        } else {
                            // Collateral of an earlier fatal error: just stopped.
                            set_chunk_state(&chunk, ChunkViewState::Pending);
                        }
                        work.cancel();
                    }
                }
                // Never leave work parked with nothing running.
                if set.is_empty() && fatal.is_none() && !work.is_cancelled() {
                    if let Some(c) = parked.pop_front() {
                        spawn_worker(&mut set, &ctx, c, backoff_delay(1, None));
                    }
                }
            }
        }
    }
    debug_assert!(
        parked.is_empty() || fatal.is_some() || work.is_cancelled(),
        "transfer ended with chunks still parked"
    );

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
            return Err(OdmError::new(
                ErrorKind::Other,
                format!("incomplete: {total} of {size} bytes"),
            ));
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
        env.store
            .save_progress(id, snap.iter().map(|c| (c.idx, c.written)).collect())
            .await
    };
    r.map_err(store_err)?;
    *layout_dirty = false;
    Ok(())
}

fn emit_progress(
    env: &DownloadEnv,
    rec: &DownloadRecord,
    table: &ChunkTable,
    meter: &mut SpeedMeter,
    session: &AtomicU64,
) {
    meter.record(Instant::now(), session.load(Ordering::Relaxed));
    let bytes = table.total_written();
    let speed = meter.bps();
    let _ = env.events.send(EngineEvent::Progress(ProgressSnapshot {
        id: rec.id.clone(),
        bytes,
        total: rec.size,
        speed_bps: speed,
        eta_s: eta(rec.size, bytes, speed),
        chunks: table.views(),
        segments: None,
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
        other => {
            return Err(OdmError::new(
                ErrorKind::Other,
                format!("unsupported checksum {other}"),
            ))
        }
    }
    .map_err(io_err)?;
    let got_hex: String = got.iter().map(|b| format!("{b:02x}")).collect();
    if got_hex.eq_ignore_ascii_case(want.trim()) {
        Ok(())
    } else {
        Err(OdmError::new(
            ErrorKind::ChecksumMismatch,
            format!("expected {want}, got {got_hex}"),
        ))
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
    let final_path = place_final(part, dir, &rec.filename, rec)?;
    set_final_name(env, rec, &final_path).await?;
    if rec.size.is_none() {
        let len = std::fs::metadata(&final_path).map_err(io_err)?.len();
        env.store
            .update_probe(
                &rec.id,
                ProbeMeta {
                    size: Some(len),
                    ..Default::default()
                },
            )
            .await
            .map_err(store_err)?;
    }
    tracing::info!(id = %rec.id, path = %final_path.display(), "completed");
    Ok(final_path)
}

/// Moves `src` into `dir` under a name that is unique there (`name`, `name (1)`, ...)
/// and marks it as downloaded from the internet. Choosing the name and renaming is
/// atomic with respect to other finishing downloads.
pub(crate) fn place_final(
    src: &Path,
    dir: &Path,
    name: &str,
    rec: &DownloadRecord,
) -> Result<PathBuf, OdmError> {
    // std::fs::rename REPLACES an existing target on Windows.
    let final_path = {
        let _guard = FINISH_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let name = filename::unique_name(name, |n| dir.join(n).exists());
        let final_path = dir.join(&name);
        std::fs::rename(src, &final_path).map_err(io_err)?;
        final_path
    };
    write_mark_of_the_web(&final_path, &rec.url, rec.page_url.as_deref());
    Ok(final_path)
}

/// Stores the file name that `place_final` actually chose.
pub(crate) async fn set_final_name(
    env: &DownloadEnv,
    rec: &DownloadRecord,
    final_path: &Path,
) -> Result<(), OdmError> {
    let name = final_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| rec.filename.clone());
    if name != rec.filename {
        env.store
            .set_filename(&rec.id, name)
            .await
            .map_err(store_err)?;
    }
    Ok(())
}

/// Marks the file as downloaded from the internet (`ZoneId=3`) so Windows
/// SmartScreen and Office protected view treat it with suspicion. URLs are
/// redacted: no query, no credentials. Best-effort: file systems without
/// alternate data streams (FAT/exFAT) only log a warning.
#[cfg(windows)]
fn write_mark_of_the_web(path: &Path, url: &str, page_url: Option<&str>) {
    let mut zone = String::from("[ZoneTransfer]\r\nZoneId=3\r\n");
    if let Some(p) = page_url {
        zone.push_str(&format!("ReferrerUrl={}\r\n", odm_core::redact_url(p)));
    }
    zone.push_str(&format!("HostUrl={}\r\n", odm_core::redact_url(url)));
    let stream = format!("{}:Zone.Identifier", path.display());
    if let Err(e) = std::fs::write(stream, zone) {
        tracing::warn!("could not write Mark of the Web: {e}");
    }
}

#[cfg(not(windows))]
fn write_mark_of_the_web(_path: &Path, _url: &str, _page_url: Option<&str>) {}

#[cfg(test)]
mod tests {
    use super::header_map;

    #[test]
    fn preallocate_sets_full_size() {
        let d = tempfile::tempdir().unwrap();
        let f = std::fs::File::create(d.path().join("p")).unwrap();
        super::preallocate(&f, 1 << 30).unwrap();
        assert_eq!(f.metadata().unwrap().len(), 1 << 30);
    }

    #[cfg(windows)]
    #[test]
    fn preallocate_marks_part_file_sparse_on_windows() {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_SPARSE_FILE: u32 = 0x200;
        let d = tempfile::tempdir().unwrap();
        let f = std::fs::File::create(d.path().join("p")).unwrap();
        super::preallocate(&f, 1 << 30).unwrap();
        let attrs = f.metadata().unwrap().file_attributes();
        assert_ne!(
            attrs & FILE_ATTRIBUTE_SPARSE_FILE,
            0,
            "part file is not sparse (attrs {attrs:#x}); NTFS will zero-fill"
        );
    }

    #[test]
    fn header_map_drops_replay_unsafe_headers() {
        let pairs: Vec<(String, String)> = [
            ("Range", "bytes=0-9"),
            ("If-Range", "x"),
            ("Content-Length", "5"),
            ("HOST", "h"),
            ("connection", "close"),
            ("Transfer-Encoding", "chunked"),
            ("Accept-Encoding", "gzip"),
            ("Cookie", "a=b"),
            ("Referer", "http://r/"),
            ("User-Agent", "ua"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let m = header_map(&pairs);
        let mut names: Vec<_> = m.keys().map(|k| k.as_str().to_string()).collect();
        names.sort();
        assert_eq!(names, ["cookie", "referer", "user-agent"]);
    }
}
