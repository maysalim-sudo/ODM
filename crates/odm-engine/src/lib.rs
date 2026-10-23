//! ODM download engine.
pub mod chunks;
pub mod limiter;
pub mod retry;

pub use limiter::SpeedLimiter;
pub mod probe;
pub use probe::{probe, ProbeResult};
pub mod config;
pub mod events;
pub mod schedule;
pub mod speed;

pub use config::{EngineConfig, ExistsChoice, ExistsPolicy};
pub use events::{ChunkView, ChunkViewState, EngineEvent, ProgressSnapshot, SegmentProgress};
pub mod worker;

pub mod download;
pub use download::{run_download, DownloadEnv, Outcome};

pub mod hls;
pub mod manager;
pub mod site;
pub mod toolfind;
pub mod torrent;
pub use torrent::{TorrentAdded, TorrentRequest};
pub use manager::{AddRequest, ManagerHandle, SiteRequest, StreamRequest};
