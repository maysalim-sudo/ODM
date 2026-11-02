//! ODM download engine.
pub mod chunks;
pub mod convert;
pub mod limiter;
pub mod retry;

pub use limiter::SpeedLimiter;
pub mod probe;
pub use probe::{probe, ProbeResult};
pub mod config;
pub mod events;
pub mod schedule;
pub mod speed;

pub use config::{apply_override, EngineConfig, ExistsChoice, ExistsPolicy, VideoOverride};
pub use events::{
    ChunkView, ChunkViewState, EngineEvent, ProgressSnapshot, SegmentProgress, TorrentProgress,
};
pub mod worker;

pub mod download;
pub use download::{run_download, DownloadEnv, Outcome};

pub mod hls;
pub mod manager;
pub mod site;
pub mod toolfind;
pub mod torrent;
pub use manager::{AddRequest, ManagerHandle, SiteRequest, StreamRequest};
pub use torrent::{TorrentAdded, TorrentRequest};
