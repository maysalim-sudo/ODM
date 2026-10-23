use odm_core::DownloadState;
use serde::Serialize;

/// Per-chunk state for the UI chunk map (spec 5.7, 9). Not persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkViewState {
    /// Not running (not started yet, or stopped by a pause/cancel).
    Pending,
    /// A worker is transferring it.
    Active,
    /// Waiting out a backoff delay before its next attempt.
    Retrying,
    /// Every byte written.
    Done,
    /// Retries exhausted or a permanent error.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChunkView {
    pub idx: u32,
    pub start: u64,
    pub end: Option<u64>,
    pub written: u64,
    pub state: ChunkViewState,
}

/// Segment counters of an HLS download.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SegmentProgress {
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProgressSnapshot {
    pub id: String,
    pub bytes: u64,
    pub total: Option<u64>,
    pub speed_bps: u64,
    pub eta_s: Option<u64>,
    pub chunks: Vec<ChunkView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<SegmentProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub torrent: Option<TorrentProgress>,
}

/// Swarm figures of a torrent download.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TorrentProgress {
    pub peers_connected: u32,
    pub peers_seen: u32,
    pub up_bps: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    Added {
        id: String,
    },
    Progress(ProgressSnapshot),
    State {
        id: String,
        state: DownloadState,
        error_code: Option<String>,
        error_message: Option<String>,
    },
    Removed {
        id: String,
    },
    /// A finished download's file name is taken and the policy is "ask". The
    /// download is paused (reason "exists") until `ManagerHandle::answer_exists`.
    ExistsPrompt {
        id: String,
        name: String,
    },
    /// The queue ran at least one download to completion and now has nothing left
    /// to run (nothing queued, active or waiting for an answer).
    QueueDone,
    /// A torrent's file list is now known (`ManagerHandle::torrent_files`).
    TorrentMetadata {
        id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(e: &EngineEvent) -> String {
        serde_json::to_string(e).unwrap()
    }

    #[test]
    fn event_json_shape_is_pinned() {
        assert_eq!(
            json(&EngineEvent::Added { id: "a1".into() }),
            r#"{"type":"added","id":"a1"}"#
        );
        assert_eq!(
            json(&EngineEvent::Progress(ProgressSnapshot {
                id: "a1".into(),
                bytes: 10,
                total: Some(100),
                speed_bps: 5,
                eta_s: Some(18),
                chunks: vec![ChunkView {
                    idx: 0,
                    start: 0,
                    end: Some(99),
                    written: 10,
                    state: ChunkViewState::Active,
                }],
                segments: None,
                torrent: None,
            })),
            r#"{"type":"progress","id":"a1","bytes":10,"total":100,"speed_bps":5,"eta_s":18,"chunks":[{"idx":0,"start":0,"end":99,"written":10,"state":"active"}]}"#
        );
        assert_eq!(
            json(&EngineEvent::Progress(ProgressSnapshot {
                id: "h1".into(),
                bytes: 300,
                total: Some(1200),
                speed_bps: 50,
                eta_s: Some(18),
                chunks: vec![],
                segments: Some(SegmentProgress { done: 3, total: 12 }),
                torrent: None,
            })),
            r#"{"type":"progress","id":"h1","bytes":300,"total":1200,"speed_bps":50,"eta_s":18,"chunks":[],"segments":{"done":3,"total":12}}"#
        );
        assert_eq!(
            json(&EngineEvent::State {
                id: "a1".into(),
                state: DownloadState::Downloading,
                error_code: None,
                error_message: None,
            }),
            r#"{"type":"state","id":"a1","state":"downloading","error_code":null,"error_message":null}"#
        );
        assert_eq!(
            json(&EngineEvent::State {
                id: "a1".into(),
                state: DownloadState::Failed,
                error_code: Some("link_expired".into()),
                error_message: Some("HTTP 403".into()),
            }),
            r#"{"type":"state","id":"a1","state":"failed","error_code":"link_expired","error_message":"HTTP 403"}"#
        );
        assert_eq!(
            json(&EngineEvent::Removed { id: "a1".into() }),
            r#"{"type":"removed","id":"a1"}"#
        );
        assert_eq!(
            json(&EngineEvent::Progress(ProgressSnapshot {
                id: "t1".into(),
                bytes: 5,
                total: Some(10),
                speed_bps: 2,
                eta_s: Some(3),
                chunks: vec![],
                segments: None,
                torrent: Some(TorrentProgress {
                    peers_connected: 3,
                    peers_seen: 9,
                    up_bps: 7,
                }),
            })),
            r#"{"type":"progress","id":"t1","bytes":5,"total":10,"speed_bps":2,"eta_s":3,"chunks":[],"torrent":{"peers_connected":3,"peers_seen":9,"up_bps":7}}"#
        );
        assert_eq!(
            json(&EngineEvent::TorrentMetadata { id: "t1".into() }),
            r#"{"type":"torrent_metadata","id":"t1"}"#
        );
        assert_eq!(
            json(&EngineEvent::ExistsPrompt {
                id: "a1".into(),
                name: "f.zip".into()
            }),
            r#"{"type":"exists_prompt","id":"a1","name":"f.zip"}"#
        );
        assert_eq!(json(&EngineEvent::QueueDone), r#"{"type":"queue_done"}"#);
    }

    #[test]
    fn chunk_state_json_names() {
        let names: Vec<String> = [
            ChunkViewState::Pending,
            ChunkViewState::Active,
            ChunkViewState::Retrying,
            ChunkViewState::Done,
            ChunkViewState::Failed,
        ]
        .iter()
        .map(|s| serde_json::to_string(s).unwrap())
        .collect();
        assert_eq!(
            names,
            [
                r#""pending""#,
                r#""active""#,
                r#""retrying""#,
                r#""done""#,
                r#""failed""#
            ]
        );
    }
}
