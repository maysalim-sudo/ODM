//! Downloads the segments of an HLS stream into a temp directory.
use futures_util::StreamExt;
use odm_core::{DownloadState, ErrorKind, OdmError};
use odm_store::{DownloadRecord, HlsMeta, ProbeMeta, SegmentRecord, Store, StoreError};
use reqwest::header::{self, HeaderMap};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::crypto::{decrypt_aes128, iv_for};
use super::merge::{merge, resolve_ffmpeg};
use super::playlist::{best_variant, parse, Key, KeyMethod, MediaPlaylist, Playlist};
use crate::download::{header_map, place_final, set_final_name, transition, DownloadEnv, Outcome};
use crate::events::{EngineEvent, ProgressSnapshot, SegmentProgress};
use crate::retry::{backoff_delay, classify_status, http_err, io_err, parse_retry_after};
use crate::speed::{eta, SpeedMeter};
use crate::worker::ChunkFailure;
use crate::SpeedLimiter;

/// `<save_dir>/.odm-hls-<id>`: where segments live until the merge.
pub fn temp_dir(rec: &DownloadRecord) -> PathBuf {
    Path::new(&rec.save_dir).join(format!(".odm-hls-{}", rec.id))
}

fn other(msg: &str) -> OdmError {
    OdmError::new(ErrorKind::Other, msg)
}

fn store_err(e: StoreError) -> OdmError {
    OdmError::new(ErrorKind::Other, format!("store: {e}"))
}

fn cancelled() -> OdmError {
    OdmError::new(ErrorKind::Cancelled, "cancelled")
}

pub async fn run_hls(env: &DownloadEnv, id: &str, cancel: CancellationToken) -> Outcome {
    match run_inner(env, id, &cancel).await {
        Ok(path) => Outcome::Completed { path },
        Err(_) if cancel.is_cancelled() => Outcome::Stopped,
        Err(e) => Outcome::Failed(e),
    }
}

/// HTTP access shared by every request of one run.
#[derive(Clone)]
struct Fetch {
    client: reqwest::Client,
    headers: HeaderMap,
    global: Arc<SpeedLimiter>,
    local: Option<Arc<SpeedLimiter>>,
    cancel: CancellationToken,
    /// Bytes received during this run (for the speed meter).
    session: Arc<AtomicU64>,
    max_attempts: u32,
}

impl Fetch {
    async fn once(&self, url: &str, started: bool) -> Result<Vec<u8>, ChunkFailure> {
        let resp = tokio::select! {
            _ = self.cancel.cancelled() => return Err(cancelled().into()),
            r = self.client.get(url).headers(self.headers.clone()).send() => {
                r.map_err(|e| ChunkFailure::from(http_err(e)))?
            }
        };
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let retry_after = resp
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            return Err(ChunkFailure {
                error: OdmError::new(classify_status(status, started), format!("HTTP {status}")),
                retry_after,
            });
        }
        let mut buf = Vec::new();
        let mut stream = resp.bytes_stream();
        loop {
            let next = tokio::select! {
                _ = self.cancel.cancelled() => return Err(cancelled().into()),
                n = stream.next() => n,
            };
            let piece = match next {
                None => break,
                Some(Err(e)) => return Err(http_err(e).into()),
                Some(Ok(p)) => p,
            };
            tokio::select! {
                _ = self.cancel.cancelled() => return Err(cancelled().into()),
                _ = async {
                    self.global.acquire(piece.len()).await;
                    if let Some(l) = &self.local { l.acquire(piece.len()).await; }
                } => {}
            }
            self.session
                .fetch_add(piece.len() as u64, Ordering::Relaxed);
            buf.extend_from_slice(&piece);
        }
        Ok(buf)
    }

    /// One whole body; transient failures are retried with backoff.
    async fn get(&self, url: &str, started: bool) -> Result<Vec<u8>, OdmError> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.once(url, started).await {
                Ok(b) => return Ok(b),
                Err(f) if f.error.kind.is_transient() && attempt < self.max_attempts => {
                    tokio::select! {
                        _ = self.cancel.cancelled() => return Err(cancelled()),
                        _ = tokio::time::sleep(backoff_delay(attempt, f.retry_after)) => {}
                    }
                }
                Err(f) => return Err(f.error),
            }
        }
    }
}

async fn fetch_media_playlist(
    env: &DownloadEnv,
    id: &str,
    fetch: &Fetch,
    mut variant_url: String,
) -> Result<MediaPlaylist, OdmError> {
    let unreadable = || other("The playlist could not be read.");
    for level in 0..2 {
        let body = fetch.get(&variant_url, false).await?;
        let text = String::from_utf8_lossy(&body);
        match parse(&text, &variant_url).map_err(|_| unreadable())? {
            Playlist::Media(m) => return Ok(m),
            Playlist::Master(v) if level == 0 => {
                let best = best_variant(&v).ok_or_else(unreadable)?;
                variant_url = best.url.clone();
                env.store
                    .set_hls_meta(
                        id,
                        HlsMeta {
                            variant_url: variant_url.clone(),
                            key_url: None,
                            iv: None,
                            map_url: None,
                        },
                    )
                    .await
                    .map_err(store_err)?;
            }
            Playlist::Master(_) => return Err(unreadable()),
        }
    }
    Err(unreadable())
}

/// Fails fast on streams ODM cannot download, in a fixed order.
fn check_supported(m: &MediaPlaylist) -> Result<(), OdmError> {
    if !m.ended {
        return Err(other("Live streams are not supported yet."));
    }
    let methods = || {
        m.segments
            .iter()
            .filter_map(|s| s.key.as_ref())
            .map(|k| &k.method)
    };
    if methods().any(|k| *k == KeyMethod::SampleAes) {
        return Err(other(
            "This stream uses SAMPLE-AES encryption, which ODM can't download.",
        ));
    }
    if methods().any(|k| matches!(k, KeyMethod::Other(_))) {
        return Err(other("This stream's encryption is not supported."));
    }
    if m.segments.iter().any(|s| s.byte_range.is_some()) {
        return Err(other("Byte-range playlists are not supported yet."));
    }
    if m.segments.is_empty() {
        return Err(other("The playlist has no video segments."));
    }
    Ok(())
}

fn seg_file(dir: &Path, idx: u32, ext: &str) -> PathBuf {
    dir.join(format!("seg_{idx:06}.{ext}"))
}

fn write_atomic(dest: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    std::fs::write(&part, data)?;
    std::fs::rename(&part, dest)
}

struct SegCtx {
    store: Store,
    id: String,
    fetch: Fetch,
    dir: PathBuf,
    ext: &'static str,
    keys: tokio::sync::Mutex<HashMap<String, [u8; 16]>>,
}

impl SegCtx {
    /// Each key URI is fetched once per run.
    async fn key_bytes(&self, uri: &str) -> Result<[u8; 16], OdmError> {
        let mut keys = self.keys.lock().await;
        if let Some(k) = keys.get(uri) {
            return Ok(*k);
        }
        let body = self.fetch.get(uri, true).await?;
        let key: [u8; 16] = body
            .try_into()
            .map_err(|_| other("The stream's key could not be read."))?;
        keys.insert(uri.to_string(), key);
        Ok(key)
    }
}

struct SegJob {
    idx: u32,
    url: String,
    seq: u64,
    key: Option<Key>,
}

async fn do_segment(ctx: Arc<SegCtx>, job: SegJob) -> Result<u64, OdmError> {
    let mut data = ctx.fetch.get(&job.url, true).await?;
    let mut decrypt = None;
    if let Some(key) = &job.key {
        let uri = key
            .uri
            .as_deref()
            .ok_or_else(|| other("The stream's key could not be read."))?;
        decrypt = Some((ctx.key_bytes(uri).await?, iv_for(key, job.seq)));
    }
    let dest = seg_file(&ctx.dir, job.idx, ctx.ext);
    let len = tokio::task::spawn_blocking(move || -> Result<u64, OdmError> {
        if let Some((k, iv)) = decrypt {
            data = decrypt_aes128(&k, &iv, &data)
                .map_err(|_| other("The segment could not be decrypted."))?;
        }
        write_atomic(&dest, &data).map_err(io_err)?;
        Ok(data.len() as u64)
    })
    .await
    .map_err(|e| OdmError::new(ErrorKind::Other, format!("segment task failed: {e}")))??;
    ctx.store
        .mark_segment_done(&ctx.id, job.idx, len)
        .await
        .map_err(store_err)?;
    Ok(len)
}

fn emit_progress(
    env: &DownloadEnv,
    id: &str,
    meter: &mut SpeedMeter,
    session: &AtomicU64,
    done: u32,
    total: u32,
    bytes: u64,
) {
    meter.record(Instant::now(), session.load(Ordering::Relaxed));
    let speed = meter.bps();
    let est = (done > 0).then(|| (bytes as u128 * total as u128 / done as u128) as u64);
    let _ = env.events.send(EngineEvent::Progress(ProgressSnapshot {
        id: id.to_string(),
        bytes,
        total: est,
        speed_bps: speed,
        eta_s: eta(est, bytes, speed),
        chunks: vec![],
        segments: Some(SegmentProgress { done, total }),
    }));
}

async fn run_inner(
    env: &DownloadEnv,
    id: &str,
    cancel: &CancellationToken,
) -> Result<PathBuf, OdmError> {
    let rec = env
        .store
        .get_download(id)
        .await
        .map_err(store_err)?
        .ok_or_else(|| other("download not found"))?;
    transition(env, id, DownloadState::Probing, None).await;

    let work = cancel.child_token();
    let session = Arc::new(AtomicU64::new(0));
    let fetch = Fetch {
        client: env.client.clone(),
        headers: header_map(&env.store.get_headers(id).await.map_err(store_err)?),
        global: env.limiter.clone(),
        local: rec.speed_limit.map(|b| Arc::new(SpeedLimiter::new(b))),
        cancel: work.clone(),
        session: session.clone(),
        max_attempts: env.config.max_attempts,
    };

    let variant_url = env
        .store
        .get_hls_meta(id)
        .await
        .map_err(store_err)?
        .map(|m| m.variant_url)
        .unwrap_or_else(|| rec.url.clone());
    let media = fetch_media_playlist(env, id, &fetch, variant_url).await?;
    check_supported(&media)?;

    let dir = temp_dir(&rec);
    let wanted: Vec<SegmentRecord> = media
        .segments
        .iter()
        .enumerate()
        .map(|(i, s)| SegmentRecord {
            idx: i as u32 + 1,
            url: s.url.clone(),
            duration_ms: s.duration_ms,
            state: "queued".into(),
            bytes: 0,
        })
        .collect();
    let mut stored = env.store.get_segments(id).await.map_err(store_err)?;
    let same =
        stored.len() == wanted.len() && stored.iter().zip(&wanted).all(|(a, b)| a.url == b.url);
    if stored.is_empty() || !same {
        if !stored.is_empty() {
            tracing::warn!(%id, "playlist changed; discarding segment progress");
            let _ = std::fs::remove_dir_all(&dir);
        }
        env.store
            .replace_segments(id, wanted.clone())
            .await
            .map_err(store_err)?;
        stored = wanted;
    }

    transition(env, id, DownloadState::Downloading, None).await;
    std::fs::create_dir_all(&dir).map_err(io_err)?;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if e.path().extension().is_some_and(|x| x == "part") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    tracing::info!(%id, url = %odm_core::redact_url(&rec.url), segments = stored.len(), "downloading HLS");

    if let Some(map) = &media.map_url {
        let init = dir.join("init.mp4");
        if !init.exists() {
            let data = fetch.get(map, true).await?;
            write_atomic(&init, &data).map_err(io_err)?;
        }
    }

    let ext: &'static str = if media.map_url.is_some() { "m4s" } else { "ts" };
    let total = stored.len() as u32;
    let mut done: u32 = 0;
    let mut done_bytes: u64 = 0;
    let mut todo: VecDeque<SegJob> = VecDeque::new();
    let mut reset = false;
    for (seg, parsed) in stored.iter_mut().zip(&media.segments) {
        let on_disk = std::fs::metadata(seg_file(&dir, seg.idx, ext))
            .ok()
            .map(|m| m.len());
        if seg.state == "done" && on_disk == Some(seg.bytes) {
            done += 1;
            done_bytes += seg.bytes;
        } else {
            if seg.state == "done" {
                seg.state = "queued".into();
                seg.bytes = 0;
                reset = true;
            }
            todo.push_back(SegJob {
                idx: seg.idx,
                url: seg.url.clone(),
                seq: parsed.seq,
                key: parsed.key.clone(),
            });
        }
    }
    if reset {
        env.store
            .replace_segments(id, stored.clone())
            .await
            .map_err(store_err)?;
    }

    let ctx = Arc::new(SegCtx {
        store: env.store.clone(),
        id: id.to_string(),
        fetch,
        dir: dir.clone(),
        ext,
        keys: tokio::sync::Mutex::new(HashMap::new()),
    });
    let max_conn = rec.max_connections.clamp(1, 32) as usize;
    let mut set: JoinSet<Result<u64, OdmError>> = JoinSet::new();
    let mut meter = SpeedMeter::new(Duration::from_secs(5));
    let mut tick = tokio::time::interval(env.config.progress_interval);
    let mut fatal: Option<OdmError> = None;

    loop {
        while fatal.is_none() && !work.is_cancelled() && set.len() < max_conn {
            let Some(job) = todo.pop_front() else { break };
            set.spawn(do_segment(ctx.clone(), job));
        }
        if set.is_empty() {
            break;
        }
        tokio::select! {
            _ = tick.tick() => {
                emit_progress(env, id, &mut meter, &session, done, total, done_bytes);
            }
            joined = set.join_next() => {
                let Some(joined) = joined else { break };
                let res = joined
                    .map_err(|e| OdmError::new(ErrorKind::Other, format!("worker panicked: {e}")))
                    .and_then(|r| r);
                match res {
                    Ok(len) => {
                        done += 1;
                        done_bytes += len;
                    }
                    Err(e) if e.kind == ErrorKind::Cancelled => {}
                    Err(e) => {
                        if fatal.is_none() {
                            fatal = Some(e);
                        }
                        work.cancel();
                    }
                }
            }
        }
    }
    emit_progress(env, id, &mut meter, &session, done, total, done_bytes);

    if let Some(e) = fatal {
        return Err(e);
    }
    if cancel.is_cancelled() {
        return Err(cancelled());
    }

    transition(env, id, DownloadState::Merging, None).await;
    let ffmpeg = resolve_ffmpeg(&env.config)
        .ok_or_else(|| other("FFmpeg not found — set its path in Settings."))?;
    let out = merge(&ffmpeg, &dir, total, media.map_url.is_some(), cancel).await?;
    let final_path = place_final(&out, Path::new(&rec.save_dir), &rec.filename, &rec)?;
    set_final_name(env, &rec, &final_path).await?;
    let len = std::fs::metadata(&final_path).map_err(io_err)?.len();
    env.store
        .update_probe(
            id,
            ProbeMeta {
                size: Some(len),
                ..Default::default()
            },
        )
        .await
        .map_err(store_err)?;
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        tracing::warn!(%id, "could not remove the HLS temp dir: {e}");
    }
    Ok(final_path)
}
