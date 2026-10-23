//! Torrent downloads: adding them, their metadata, and (in `run`) downloading them.
pub mod progress;
pub mod run;
use odm_store::TorrentFileRecord;
use odm_torrent::{TorrentMeta, TorrentSettings, TorrentSource};
use std::path::{Path, PathBuf};

use crate::download::DownloadEnv;
use crate::{EngineConfig, EngineEvent};
use odm_core::filename;
use odm_store::TorrentMetaRecord;

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

/// Saves metadata peers sent for a torrent waiting for it: the file list (all selected),
/// a `.torrent` copy as its new source, and its name. Then tells the UI.
pub(crate) async fn store_metadata(env: &DownloadEnv, id: &str, meta: &TorrentMeta) {
    let store = &env.store;
    let (Ok(Some(rec)), Ok(Some(tm))) = (store.get_download(id).await, store.get_torrent_meta(id).await) else {
        return;
    };
    if tm.has_meta {
        return;
    }
    let Some(hash) = rec.info_hash.clone() else {
        return;
    };
    // Kept as a .torrent so the download does not ask peers for it again.
    let path = stored_torrent(&env.config, &hash);
    let source = match write_torrent(&path, &meta.torrent_bytes) {
        Ok(()) => path.to_string_lossy().into_owned(),
        Err(e) => {
            tracing::warn!(id = %id, "saving torrent metadata: {e}");
            tm.source.clone()
        }
    };
    let _ = store
        .put_torrent_meta(TorrentMetaRecord { source, ..tm })
        .await;
    if store
        .replace_torrent_files(id, file_records(meta))
        .await
        .is_err()
    {
        return;
    }
    let name = filename::sanitize(&meta.name);
    if !rec.filename_locked && rec.filename != name {
        let _ = store.set_filename(id, name).await;
    }
    let _ = env.events.send(EngineEvent::TorrentMetadata { id: id.to_string() });
}

/// `path` (a torrent's `/`-separated relative path) under `out`, or None unless every part
/// is a plain name: no drive, root, `.` or `..` can take it outside `out`.
pub(crate) fn inside(out: &Path, path: &str) -> Option<PathBuf> {
    let mut p = out.to_path_buf();
    for part in path.split('/') {
        let mut c = Path::new(part).components();
        match (c.next(), c.next()) {
            (Some(std::path::Component::Normal(n)), None) => p.push(n),
            _ => return None,
        }
    }
    Some(p)
}

/// Deletes a torrent's own files under `out`, then the folders they leave empty, up to
/// `out` itself when `own_folder` (it was made for this torrent). Anything else in those
/// folders stays, and so do the folders that still hold it.
pub(crate) fn delete_files(out: &Path, files: &[TorrentFileRecord], own_folder: bool) {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for f in files {
        let Some(p) = inside(out, &f.path) else { continue };
        let _ = std::fs::remove_file(&p);
        let mut d = p.parent().map(Path::to_path_buf);
        while let Some(dir) = d {
            if dir == out || !dir.starts_with(out) {
                break;
            }
            if !dirs.contains(&dir) {
                dirs.push(dir.clone());
            }
            d = dir.parent().map(Path::to_path_buf);
        }
    }
    // Deepest first; remove_dir only removes empty folders.
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for d in dirs {
        let _ = std::fs::remove_dir(d);
    }
    if own_folder {
        let _ = std::fs::remove_dir(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside_rejects_escapes() {
        let out = Path::new("C:/dl/Foo");
        assert_eq!(inside(out, "a/b.txt"), Some(out.join("a").join("b.txt")));
        for bad in ["../x", "a/../../x", "/abs", "c:x", "C:/x", "a//b", ".", "a/./b"] {
            assert_eq!(inside(out, bad), None, "{bad}");
        }
    }
}
