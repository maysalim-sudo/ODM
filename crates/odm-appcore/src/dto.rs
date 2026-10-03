use crate::messages::user_message;
use crate::offers::OfferDto;
use odm_core::{DownloadState, ErrorKind, OdmError};
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

/// The stored failure text, rewritten in plain language when its code is known.
fn failure_message(r: &DownloadRecord) -> Option<String> {
    let stored = r.error_msg.clone();
    match r.error_kind.as_deref().and_then(ErrorKind::from_code) {
        Some(kind) => Some(user_message(&OdmError::new(
            kind,
            stored.unwrap_or_default(),
        ))),
        None => stored,
    }
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
        error_message: failure_message(r),
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
    /// A browser download matches a paused or failed one; ask whether to continue it.
    ReuseOffer {
        offer: OfferDto,
    },
}

/// The download in `ev`, if this event says it just completed.
pub fn completed_download(ev: &UiEvent) -> Option<&DownloadDto> {
    match ev {
        UiEvent::Upsert { download } if download.state == DownloadState::Completed => {
            Some(download.as_ref())
        }
        _ => None,
    }
}

/// Remembers which downloads have already been reported as completed, so a
/// download that the bridge reports as completed more than once (a fast one is
/// already complete when its earlier events are looked up) notifies only once.
#[derive(Debug, Default)]
pub struct CompletionTracker {
    notified: std::collections::HashSet<String>,
}

impl CompletionTracker {
    /// The download in `ev` only the first time it is seen completed. An upsert in
    /// any other state, or a removal, forgets the id so a later completion counts again.
    pub fn newly_completed<'a>(&mut self, ev: &'a UiEvent) -> Option<&'a DownloadDto> {
        match ev {
            UiEvent::Upsert { download } if download.state == DownloadState::Completed => {
                if self.notified.insert(download.id.clone()) {
                    Some(download.as_ref())
                } else {
                    None
                }
            }
            UiEvent::Upsert { download } => {
                self.notified.remove(&download.id);
                None
            }
            UiEvent::Removed { id } => {
                self.notified.remove(id);
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tracker_tests {
    use super::*;

    fn up(id: &str, state: DownloadState) -> UiEvent {
        let mut r = DownloadRecord::new_file(id, "https://e.com/f", "f", "C:/dl");
        r.state = state;
        UiEvent::Upsert {
            download: Box::new(to_dto(&r, 0)),
        }
    }

    #[test]
    fn notifies_once_per_completion() {
        let mut t = CompletionTracker::default();
        let done = up("a", DownloadState::Completed);
        assert!(t.newly_completed(&done).is_some());
        assert!(t.newly_completed(&done).is_none());
        assert!(t
            .newly_completed(&up("b", DownloadState::Completed))
            .is_some());
    }

    #[test]
    fn removed_or_requeued_allows_another_notification() {
        let mut t = CompletionTracker::default();
        let done = up("a", DownloadState::Completed);
        assert!(t.newly_completed(&done).is_some());
        assert!(t
            .newly_completed(&UiEvent::Removed { id: "a".into() })
            .is_none());
        assert!(t.newly_completed(&done).is_some());
        assert!(t.newly_completed(&up("a", DownloadState::Queued)).is_none());
        assert!(t.newly_completed(&done).is_some());
        assert!(t.newly_completed(&UiEvent::Resync).is_none());
    }
}

#[cfg(test)]
mod message_tests {
    use super::*;

    fn failed(kind: Option<&str>, msg: &str) -> DownloadRecord {
        let mut r = DownloadRecord::new_file("a", "https://e.com/f", "f", "C:/dl");
        r.state = DownloadState::Failed;
        r.error_kind = kind.map(String::from);
        r.error_msg = Some(msg.into());
        r
    }

    #[test]
    fn link_expired_uses_the_spec_text() {
        let dto = to_dto(&failed(Some("link_expired"), "HTTP 403"), 0);
        assert_eq!(
            dto.error_message.as_deref(),
            Some("Link expired — re-send it from the browser.")
        );
        assert_eq!(dto.error_code.as_deref(), Some("link_expired"));
    }

    #[test]
    fn disk_full_is_plain() {
        let dto = to_dto(
            &failed(
                Some("disk_full"),
                "There is not enough space on the disk. (os error 112)",
            ),
            0,
        );
        assert_eq!(dto.error_message.as_deref(), Some("The disk is full."));
    }

    #[test]
    fn unknown_code_keeps_the_stored_message() {
        let dto = to_dto(&failed(Some("weird"), "raw text"), 0);
        assert_eq!(dto.error_message.as_deref(), Some("raw text"));
        let dto = to_dto(&failed(None, "raw text"), 0);
        assert_eq!(dto.error_message.as_deref(), Some("raw text"));
    }
}
