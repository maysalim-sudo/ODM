use odm_core::DownloadState;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChunkView {
    pub start: u64,
    pub end: Option<u64>,
    pub written: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProgressSnapshot {
    pub id: String,
    pub bytes: u64,
    pub total: Option<u64>,
    pub speed_bps: u64,
    pub eta_s: Option<u64>,
    pub chunks: Vec<ChunkView>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    Added { id: String },
    Progress(ProgressSnapshot),
    State {
        id: String,
        state: DownloadState,
        error_code: Option<String>,
        error_message: Option<String>,
    },
    Removed { id: String },
}
