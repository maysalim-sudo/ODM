//! Torrent downloads: adding them, their metadata, and (in `run`) downloading them.
use odm_store::TorrentFileRecord;
use odm_torrent::{TorrentMeta, TorrentSettings, TorrentSource};
use std::path::{Path, PathBuf};

use crate::EngineConfig;

/// A magnet link or `.torrent` to add. It is added paused, waiting for the user's file choice.
#[derive(Debug, Clone)]
pub struct TorrentRequest {
    pub source: TorrentSource,
    pub save_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TorrentAdded {
    pub id: String,
    /// The torrent was already in the list; its new trackers were merged into that row.
    pub existing: bool,
}

/// Where ODM keeps the `.torrent` of a download: one the user gave, or one rebuilt from
/// the metadata peers sent.
pub(crate) fn stored_torrent(cfg: &EngineConfig, info_hash: &str) -> PathBuf {
    cfg.torrent
        .state_dir
        .join("files")
        .join(format!("{info_hash}.torrent"))
}

pub(crate) fn write_torrent(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, bytes)
}

/// Every file selected: what a new torrent starts with.
pub(crate) fn file_records(meta: &TorrentMeta) -> Vec<TorrentFileRecord> {
    meta.files
        .iter()
        .map(|f| TorrentFileRecord {
            idx: f.idx,
            path: f.path.clone(),
            size: f.size,
            selected: true,
            done: 0,
        })
        .collect()
}

/// The client's settings: the configured ones, with ODM's global download limit now in force.
pub(crate) fn client_settings(cfg: &EngineConfig, global_bps: u64) -> TorrentSettings {
    TorrentSettings {
        download_bps: (global_bps > 0).then_some(global_bps),
        ..cfg.torrent.clone()
    }
}
