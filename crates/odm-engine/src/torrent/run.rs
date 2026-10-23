//! Downloads one torrent: its selected files, until they are complete. No seeding: a
//! finished torrent is stopped and taken out of the session.
use odm_core::{DownloadState, ErrorKind, OdmError};
use odm_store::{DownloadRecord, TorrentFileRecord};
use odm_torrent::{InfoHash, StartTorrent, TorrentError, TorrentSource, TorrentStats};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use super::progress::Monotonic;
use super::store_metadata;
use crate::download::{transition, DownloadEnv, Outcome};
use crate::events::{EngineEvent, ProgressSnapshot, TorrentProgress};

fn other(msg: impl Into<String>) -> OdmError {
    OdmError::new(ErrorKind::Other, msg)
}

fn cancelled() -> OdmError {
    OdmError::new(ErrorKind::Cancelled, "cancelled")
}

fn client_err(e: TorrentError) -> OdmError {
    match e {
        TorrentError::Io(m) => OdmError::new(ErrorKind::Io, m),
        TorrentError::Cancelled => cancelled(),
        e => other(e.to_string()),
    }
}

/// The folder librqbit writes into: the save folder for a single file, else a folder
/// named after the torrent inside it.
pub(crate) fn output_dir(rec: &DownloadRecord, files: &[TorrentFileRecord]) -> PathBuf {
    let dir = PathBuf::from(&rec.save_dir);
    if files.len() == 1 {
        dir
    } else {
        dir.join(&rec.filename)
    }
}

pub async fn run_torrent(env: &DownloadEnv, id: &str, cancel: CancellationToken) -> Outcome {
    let hash = match env.store.get_download(id).await {
        Ok(Some(r)) => r.info_hash.as_deref().and_then(InfoHash::parse),
        _ => None,
    };
    match run_inner(env, id, &cancel).await {
        Ok(path) => Outcome::Completed { path },
        Err(e) => {
            // Leave nothing transferring: a stopped or failed torrent stays paused in the session.
            if let Some(h) = &hash {
                let _ = env.torrents.pause(h).await;
            }
            if cancel.is_cancelled() {
                Outcome::Stopped
            } else {
                Outcome::Failed(e)
            }
        }
    }
}

async fn run_inner(env: &DownloadEnv, id: &str, cancel: &CancellationToken) -> Result<PathBuf, OdmError> {
    let store_err = |e: odm_store::StoreError| other(format!("store: {e}"));
    let rec = env.store.get_download(id).await.map_err(store_err)?.ok_or_else(|| other("download not found"))?;
    let hash = rec
        .info_hash
        .as_deref()
        .and_then(InfoHash::parse)
        .ok_or_else(|| other("torrent without an info hash"))?;
    transition(env, id, DownloadState::Probing, None).await;

    let mut tm = env.store.get_torrent_meta(id).await.map_err(store_err)?.ok_or_else(|| other("torrent source missing"))?;
    if !tm.has_meta {
        // "Download all" before the file list arrived: fetch it first, in this queue slot.
        let meta = env
            .torrents
            .fetch_metadata(&TorrentSource::Magnet(tm.source.clone()), cancel.child_token())
            .await
            .map_err(client_err)?;
        store_metadata(env, id, &meta).await?;
        tm = env.store.get_torrent_meta(id).await.map_err(store_err)?.ok_or_else(|| other("torrent source missing"))?;
    }
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let rec = env.store.get_download(id).await.map_err(store_err)?.ok_or_else(|| other("download not found"))?;
    let files = env.store.get_torrent_files(id).await.map_err(store_err)?;
    let selected: Vec<usize> = files.iter().filter(|f| f.selected).map(|f| f.idx).collect();
    let out = output_dir(&rec, &files);
    let source = if tm.source.starts_with("magnet:") {
        TorrentSource::Magnet(tm.source.clone())
    } else {
        let bytes = std::fs::read(&tm.source).map_err(|e| OdmError::new(ErrorKind::Io, e.to_string()))?;
        TorrentSource::Bytes(bytes.into())
    };
    env.torrents
        .start(StartTorrent {
            info_hash: hash.clone(),
            source,
            save_dir: out.clone(),
            only_files: Some(selected),
            trackers: tm.trackers.clone(),
            download_bps: rec.speed_limit.filter(|&b| b > 0),
        })
        .await
        .map_err(client_err)?;

    let single = (files.len() == 1).then(|| files[0].path.clone());
    let mut mono = Monotonic::default();
    let mut state = DownloadState::Probing;
    let mut last_persist = Instant::now();
    let every = env.config.progress_interval;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Err(cancelled()),
            _ = tokio::time::sleep(every) => {}
        }
        let Some(st) = env.torrents.stats(&hash) else {
            continue;
        };
        if let Some(e) = &st.error {
            return Err(OdmError::new(ErrorKind::Io, e.clone()));
        }
        if state == DownloadState::Probing && st.has_meta && (!st.checking || st.finished) {
            transition(env, id, DownloadState::Downloading, None).await;
            state = DownloadState::Downloading;
        }
        let bytes = mono.report(st.selected_done, st.selected_total);
        emit_progress(env, id, &st, bytes);
        if st.finished || last_persist.elapsed() >= env.config.persist_interval.max(Duration::from_secs(5)) {
            let done = if st.finished {
                // Every selected file is complete, whatever the client's per-file figures say.
                let now = env.store.get_torrent_files(id).await.unwrap_or_default();
                now.iter().filter(|f| f.selected).map(|f| (f.idx, f.size)).collect()
            } else {
                st.files.iter().map(|f| (f.idx, f.done)).collect()
            };
            let _ = env.store.save_torrent_progress(id, done).await;
            last_persist = Instant::now();
        }
        if st.finished && state == DownloadState::Downloading {
            env.torrents.pause(&hash).await.map_err(client_err)?;
            env.torrents.remove(&hash, false).await.map_err(client_err)?;
            return Ok(match &single {
                Some(p) => out.join(p),
                None => out,
            });
        }
    }
}

fn emit_progress(env: &DownloadEnv, id: &str, st: &TorrentStats, bytes: u64) {
    let left = st.selected_total.saturating_sub(bytes);
    let eta_s = (st.down_bps > 0).then(|| left / st.down_bps);
    let _ = env.events.send(EngineEvent::Progress(ProgressSnapshot {
        id: id.to_string(),
        bytes,
        total: Some(st.selected_total),
        speed_bps: st.down_bps,
        eta_s,
        chunks: vec![],
        segments: None,
        torrent: Some(TorrentProgress {
            peers_connected: st.peers_connected,
            peers_seen: st.peers_seen,
            up_bps: st.up_bps,
        }),
    }));
}
