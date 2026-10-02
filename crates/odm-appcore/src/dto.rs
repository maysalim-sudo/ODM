use odm_core::DownloadState;
use odm_engine::{ChunkView, ChunkViewState, ProgressSnapshot};
use odm_store::{ChunkRecord, DownloadRecord};
use serde::Serialize;

/// One row of the download list, as the UI sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DownloadDto {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub save_dir: String,
    pub size: Option<u64>,
    pub bytes: u64,
    pub mime: Option<String>,
    pub category: String,
    pub state: DownloadState,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub max_connections: u32,
    pub kind: String,
    pub page_url: Option<String>,
    pub page_title: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

pub fn to_dto(r: &DownloadRecord, bytes: u64) -> DownloadDto {
    let bytes = match (r.state, r.size) {
        (DownloadState::Completed, Some(size)) => size,
        _ => bytes,
    };
    DownloadDto {
        id: r.id.clone(),
        url: r.url.clone(),
        filename: r.filename.clone(),
        save_dir: r.save_dir.clone(),
        size: r.size,
        bytes,
        mime: r.mime.clone(),
        category: r.category.clone(),
        state: r.state,
        error_code: r.error_kind.clone(),
        error_message: r.error_msg.clone(),
        max_connections: r.max_connections,
        kind: r.kind.clone(),
        page_url: r.page_url.clone(),
        page_title: r.page_title.clone(),
        created_at: r.created_at,
        started_at: r.started_at,
        finished_at: r.finished_at,
    }
}

/// Chunk views built from stored records, for a download that is not running.
pub fn chunk_views(chunks: &[ChunkRecord]) -> Vec<ChunkView> {
    chunks
        .iter()
        .map(|c| {
            let done = matches!(c.end, Some(e) if c.start + c.written > e);
            ChunkView {
                idx: c.idx,
                start: c.start,
                end: c.end,
                written: c.written,
                state: if done {
                    ChunkViewState::Done
                } else {
                    ChunkViewState::Pending
                },
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DetailDto {
    pub download: DownloadDto,
    pub final_url: Option<String>,
    pub chunks: Vec<ChunkView>,
    /// Names only. Header values (cookies, tokens) never leave the engine.
    pub header_names: Vec<String>,
}

/// Everything the UI receives on the `odm://event` channel.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    Upsert {
        download: Box<DownloadDto>,
    },
    Removed {
        id: String,
    },
    ProgressBatch {
        items: Vec<ProgressSnapshot>,
    },
    /// The UI missed events and must reload the list.
    Resync,
}
