//! BitTorrent for ODM. The only crate that uses librqbit; everything else talks to
//! [`TorrentClient`].
pub mod parse;
mod types;

pub use parse::{magnet_info_hash, parse};
pub use types::*;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait TorrentClient: Send + Sync {
    /// Reads a magnet link or `.torrent` bytes. No network.
    fn parse(&self, source: &TorrentSource) -> Result<ParsedTorrent, TorrentError> {
        parse::parse(source)
    }

    /// The file list: immediate for a `.torrent`, fetched from peers for a magnet.
    async fn fetch_metadata(
        &self,
        source: &TorrentSource,
        cancel: CancellationToken,
    ) -> Result<TorrentMeta, TorrentError>;

    /// Starts downloading, or resumes from fastresume data.
    async fn start(&self, req: StartTorrent) -> Result<(), TorrentError>;

    async fn set_files(&self, hash: &InfoHash, selected: &[usize]) -> Result<(), TorrentError>;

    async fn pause(&self, hash: &InfoHash) -> Result<(), TorrentError>;

    /// Takes the torrent out of the session; `delete_files` also deletes its data.
    async fn remove(&self, hash: &InfoHash, delete_files: bool) -> Result<(), TorrentError>;

    fn stats(&self, hash: &InfoHash) -> Option<TorrentStats>;

    fn pieces(&self, hash: &InfoHash) -> Option<PieceMap>;

    async fn apply_settings(&self, settings: TorrentSettings) -> Result<(), TorrentError>;
}
