//! SQLite persistence. One dedicated thread owns the connection; callers
//! submit closures over a channel and await the result.
mod migrations;
mod models;
mod repo;

pub use models::{now_ms, ChunkRecord, DownloadRecord, ProbeMeta};

use odm_core::DownloadState;
use rusqlite::Connection;
use std::path::Path;
use std::sync::mpsc;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("store thread closed")]
    Closed,
}

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

#[derive(Clone)]
pub struct Store {
    tx: mpsc::Sender<Job>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self::start(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::start(Connection::open_in_memory()?)
    }

    fn start(mut conn: Connection) -> Result<Self, StoreError> {
        // In-memory DBs answer "memory" to the WAL pragma; that's fine.
        let _: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        migrations::migrate(&mut conn)?;
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("odm-store".into())
            .spawn(move || {
                for job in rx {
                    job(&mut conn);
                }
            })
            .expect("spawn store thread");
        Ok(Self { tx })
    }

    pub async fn call<R, F>(&self, f: F) -> Result<R, StoreError>
    where
        R: Send + 'static,
        F: FnOnce(&mut Connection) -> rusqlite::Result<R> + Send + 'static,
    {
        let (otx, orx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Box::new(move |c| {
                let _ = otx.send(f(c));
            }))
            .map_err(|_| StoreError::Closed)?;
        Ok(orx.await.map_err(|_| StoreError::Closed)??)
    }

    pub async fn insert_download(&self, d: DownloadRecord) -> Result<(), StoreError> {
        self.call(move |c| repo::insert_download(c, &d)).await
    }
    pub async fn get_download(&self, id: &str) -> Result<Option<DownloadRecord>, StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::get_download(c, &id)).await
    }
    pub async fn list_downloads(&self) -> Result<Vec<DownloadRecord>, StoreError> {
        self.call(|c| repo::list_downloads(c)).await
    }
    pub async fn list_by_state(&self, s: DownloadState) -> Result<Vec<DownloadRecord>, StoreError> {
        self.call(move |c| repo::list_by_state(c, s)).await
    }
    pub async fn set_state(&self, id: &str, s: DownloadState, err: Option<(String, String)>) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::set_state(c, &id, s, err)).await
    }
    pub async fn update_probe(&self, id: &str, m: ProbeMeta) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::update_probe(c, &id, &m)).await
    }
    pub async fn set_filename(&self, id: &str, name: String) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::set_filename(c, &id, &name)).await
    }
    pub async fn set_headers(&self, id: &str, h: Vec<(String, String)>) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::set_headers(c, &id, &h)).await
    }
    pub async fn get_headers(&self, id: &str) -> Result<Vec<(String, String)>, StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::get_headers(c, &id)).await
    }
    pub async fn replace_chunks(&self, id: &str, ch: Vec<ChunkRecord>) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::replace_chunks(c, &id, &ch)).await
    }
    pub async fn save_progress(&self, id: &str, p: Vec<(u32, u64)>) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::save_progress(c, &id, &p)).await
    }
    pub async fn get_chunks(&self, id: &str) -> Result<Vec<ChunkRecord>, StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::get_chunks(c, &id)).await
    }
    pub async fn delete_download(&self, id: &str) -> Result<(), StoreError> {
        let id = id.to_string();
        self.call(move |c| repo::delete_download(c, &id)).await
    }
    pub async fn reset_interrupted(&self) -> Result<usize, StoreError> {
        self.call(|c| repo::reset_interrupted(c)).await
    }
}
