use odm_core::DownloadState;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadRecord {
    pub id: String,
    pub url: String,
    pub final_url: Option<String>,
    pub filename: String,
    pub save_dir: String,
    pub size: Option<u64>,
    pub mime: Option<String>,
    pub category: String,
    pub state: DownloadState,
    pub error_kind: Option<String>,
    pub error_msg: Option<String>,
    pub priority: i64,
    pub max_connections: u32,
    pub speed_limit: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub checksum: Option<String>,
    pub kind: String,
    pub page_url: Option<String>,
    pub page_title: Option<String>,
    pub filename_locked: bool,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub auto_place: bool,
    /// Why a paused download is paused: None (by the user), "schedule" or "exists"
    /// (waiting for the user to say what to do with a file that already exists).
    pub pause_reason: Option<String>,
}

impl DownloadRecord {
    pub fn new_file(id: &str, url: &str, filename: &str, save_dir: &str) -> Self {
        Self {
            id: id.into(),
            url: url.into(),
            final_url: None,
            filename: filename.into(),
            save_dir: save_dir.into(),
            size: None,
            mime: None,
            category: "general".into(),
            state: DownloadState::Queued,
            error_kind: None,
            error_msg: None,
            priority: 0,
            max_connections: 8,
            speed_limit: None,
            etag: None,
            last_modified: None,
            checksum: None,
            kind: "file".into(),
            page_url: None,
            page_title: None,
            filename_locked: false,
            created_at: now_ms(),
            started_at: None,
            finished_at: None,
            auto_place: false,
            pause_reason: None,
        }
    }
}

impl DownloadRecord {
    /// A stream download; `url` is the playlist URL.
    pub fn new_hls(id: &str, playlist_url: &str, filename: &str, save_dir: &str) -> Self {
        Self {
            kind: "hls".into(),
            ..Self::new_file(id, playlist_url, filename, save_dir)
        }
    }
}

impl DownloadRecord {
    /// A site video fetched by yt-dlp; `url` is the page address.
    pub fn new_site(id: &str, page_url: &str, filename: &str, save_dir: &str) -> Self {
        Self {
            kind: "site".into(),
            ..Self::new_file(id, page_url, filename, save_dir)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentRecord {
    pub idx: u32,
    pub url: String,
    pub duration_ms: u64,
    /// "queued" | "done"
    pub state: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HlsMeta {
    pub variant_url: String,
    pub key_url: Option<String>,
    pub iv: Option<String>,
    pub map_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkRecord {
    pub idx: u32,
    pub start: u64,
    /// Inclusive end; None = open-ended stream of unknown length.
    pub end: Option<u64>,
    pub written: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProbeMeta {
    pub final_url: Option<String>,
    pub size: Option<u64>,
    pub mime: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub filename: Option<String>,
}
