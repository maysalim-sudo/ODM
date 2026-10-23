//! Types shared by the torrent client and its users. No librqbit types leak out of this crate.
use bytes::Bytes;
use std::net::SocketAddr;
use std::path::PathBuf;

/// What the user gave ODM: a magnet link or the bytes of a `.torrent` file.
#[derive(Debug, Clone, PartialEq)]
pub enum TorrentSource {
    Magnet(String),
    Bytes(Bytes),
}

/// A v1 info hash as 40 lowercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InfoHash(String);

impl InfoHash {
    /// Accepts 40 hex characters in any case.
    pub fn parse(hex: &str) -> Option<Self> {
        (hex.len() == 40 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| Self(hex.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for InfoHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One file inside a torrent. `idx` is librqbit's index; padding files are left out, so
/// indices can have gaps. `path` uses `/` between components.
#[derive(Debug, Clone, PartialEq)]
pub struct TorrentFile {
    pub idx: usize,
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TorrentMeta {
    pub name: String,
    pub files: Vec<TorrentFile>,
    /// The `.torrent` bytes (for a magnet, rebuilt from the metadata peers sent).
    pub torrent_bytes: Bytes,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTorrent {
    pub info_hash: InfoHash,
    pub name: Option<String>,
    pub trackers: Vec<String>,
    /// Known for a `.torrent`; None for a magnet until peers send it.
    pub meta: Option<TorrentMeta>,
}

#[derive(Debug, Clone)]
pub struct StartTorrent {
    pub info_hash: InfoHash,
    pub source: TorrentSource,
    pub save_dir: PathBuf,
    /// None downloads every file.
    pub only_files: Option<Vec<usize>>,
    pub trackers: Vec<String>,
    /// This torrent's own download limit; None = none.
    pub download_bps: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FileProgress {
    pub idx: usize,
    pub done: u64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TorrentStats {
    /// Size of the selected files.
    pub selected_total: u64,
    /// Bytes of the selected files that are verified on disk.
    pub selected_done: u64,
    pub down_bps: u64,
    pub up_bps: u64,
    pub peers_connected: u32,
    pub peers_seen: u32,
    pub files: Vec<FileProgress>,
    /// Every selected file is complete.
    pub finished: bool,
    /// Checking data already on disk.
    pub checking: bool,
    pub has_meta: bool,
    pub error: Option<String>,
}

/// Which pieces are on disk: a bitfield, most significant bit first.
#[derive(Debug, Clone, PartialEq)]
pub struct PieceMap {
    pub piece_count: u32,
    pub have: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TorrentSettings {
    pub listen_port: u16,
    pub upnp: bool,
    /// Some(0) = no upload; None = unlimited.
    pub upload_bps: Option<u64>,
    /// ODM's global download limit; None = unlimited.
    pub download_bps: Option<u64>,
    pub peer_limit: usize,
    /// `socks5://` or `socks5h://` only.
    pub socks_proxy: Option<String>,
    /// Session persistence, fastresume and DHT state.
    pub state_dir: PathBuf,
    /// Peers to try first; empty in the app, used by tests to reach a local seeder.
    pub extra_peers: Vec<SocketAddr>,
    /// No DHT, trackers, local discovery or UPnP: tests only.
    pub offline: bool,
}

impl Default for TorrentSettings {
    fn default() -> Self {
        Self {
            listen_port: 0,
            upnp: true,
            upload_bps: Some(102_400),
            download_bps: None,
            peer_limit: 100,
            socks_proxy: None,
            state_dir: PathBuf::new(),
            extra_peers: Vec::new(),
            offline: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TorrentError {
    #[error("{0}")]
    Invalid(String),
    #[error("torrent session: {0}")]
    Session(String),
    #[error("torrent not found")]
    NotFound,
    #[error("{0}")]
    Io(String),
    #[error("cancelled")]
    Cancelled,
}
