use odm_core::{category, filename, DownloadState, ErrorKind, OdmError};
use odm_store::{ChunkRecord, DownloadRecord, Store};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::download::{build_client, part_path, run_download, transition, DownloadEnv, Outcome};
use crate::{EngineConfig, EngineEvent, SpeedLimiter};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Default)]
pub struct AddRequest {
    pub url: String,
    pub filename: Option<String>,
    pub save_dir: Option<PathBuf>,
    pub category: Option<String>,
    pub headers: Vec<(String, String)>,
    pub max_connections: Option<u32>,
    pub checksum: Option<String>,
    pub page_url: Option<String>,
    pub page_title: Option<String>,
    pub priority: i64,
    pub start_paused: bool,
}

impl AddRequest {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stop {
    Pause,
    Cancel,
    Remove { delete: bool },
    Shutdown,
}

enum Cmd {
    Add(AddRequest, oneshot::Sender<Result<String, OdmError>>),
    Pause(String),
    Resume(String),
    Cancel(String, bool),
    Remove(String, bool),
    PauseAll,
    ResumeAll,
    SetSpeedLimit(u64),
    SetMaxConcurrent(usize),
    SetConfig(Box<EngineConfig>),
    Finished(String, Outcome),
    Shutdown(oneshot::Sender<()>),
    ShutdownDeadline,
}

struct Active {
    cancel: CancellationToken,
    stop: Option<Stop>,
}

struct Manager {
    env: DownloadEnv,
    tx: mpsc::Sender<Cmd>,
    active: HashMap<String, Active>,
    resume_after: HashSet<String>,
    shutdown_reply: Option<oneshot::Sender<()>>,
    max_concurrent: usize,
}

#[derive(Clone)]
pub struct ManagerHandle {
    tx: mpsc::Sender<Cmd>,
    events: broadcast::Sender<EngineEvent>,
    store: Store,
}

fn store_err(e: odm_store::StoreError) -> OdmError {
    OdmError::new(ErrorKind::Other, format!("store: {e}"))
}

fn closed() -> OdmError {
    OdmError::new(ErrorKind::Other, "engine stopped")
}

impl ManagerHandle {
    pub async fn start(config: EngineConfig, store: Store) -> Result<ManagerHandle, OdmError> {
        let config = config.sanitized();
        store.reset_interrupted().await.map_err(store_err)?;
        let (events, _) = broadcast::channel(4096);
        let env = DownloadEnv {
            store: store.clone(),
            client: build_client(&config),
            limiter: Arc::new(SpeedLimiter::new(config.speed_limit_bps)),
            events: events.clone(),
            config: config.clone(),
        };
        let (tx, rx) = mpsc::channel(256);
        let manager = Manager {
            env,
            tx: tx.clone(),
            active: HashMap::new(),
            resume_after: HashSet::new(),
            shutdown_reply: None,
            max_concurrent: config.max_concurrent.clamp(1, 10),
        };
        tokio::spawn(manager.run(rx));
        Ok(ManagerHandle { tx, events, store })
    }

    async fn send(&self, c: Cmd) -> Result<(), OdmError> {
        self.tx.send(c).await.map_err(|_| closed())
    }

    pub async fn add(&self, req: AddRequest) -> Result<String, OdmError> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::Add(req, tx)).await?;
        rx.await.map_err(|_| closed())?
    }
    pub async fn pause(&self, id: &str) -> Result<(), OdmError> {
        self.send(Cmd::Pause(id.into())).await
    }
    pub async fn resume(&self, id: &str) -> Result<(), OdmError> {
        self.send(Cmd::Resume(id.into())).await
    }
    pub async fn cancel(&self, id: &str, delete_file: bool) -> Result<(), OdmError> {
        self.send(Cmd::Cancel(id.into(), delete_file)).await
    }
    pub async fn remove(&self, id: &str, delete_file: bool) -> Result<(), OdmError> {
        self.send(Cmd::Remove(id.into(), delete_file)).await
    }
    pub async fn pause_all(&self) -> Result<(), OdmError> {
        self.send(Cmd::PauseAll).await
    }
    pub async fn resume_all(&self) -> Result<(), OdmError> {
        self.send(Cmd::ResumeAll).await
    }
    pub async fn set_speed_limit(&self, bps: u64) -> Result<(), OdmError> {
        self.send(Cmd::SetSpeedLimit(bps)).await
    }
    pub async fn set_max_concurrent(&self, n: usize) -> Result<(), OdmError> {
        self.send(Cmd::SetMaxConcurrent(n)).await
    }

    pub async fn set_config(&self, config: EngineConfig) -> Result<(), OdmError> {
        self.send(Cmd::SetConfig(Box::new(config))).await
    }

    pub async fn list(&self) -> Result<Vec<(DownloadRecord, u64)>, OdmError> {
        self.store.list_with_bytes().await.map_err(store_err)
    }

    pub async fn get(&self, id: &str) -> Result<Option<(DownloadRecord, u64)>, OdmError> {
        self.store.get_with_bytes(id).await.map_err(store_err)
    }

    pub async fn chunks(&self, id: &str) -> Result<Vec<ChunkRecord>, OdmError> {
        self.store.get_chunks(id).await.map_err(store_err)
    }

    /// Header names only: values (cookies, tokens) never leave the engine.
    pub async fn header_names(&self, id: &str) -> Result<Vec<String>, OdmError> {
        Ok(self
            .store
            .get_headers(id)
            .await
            .map_err(store_err)?
            .into_iter()
            .map(|(n, _)| n)
            .collect())
    }

    pub async fn active_count(&self) -> Result<usize, OdmError> {
        Ok(self
            .list()
            .await?
            .iter()
            .filter(|(r, _)| r.state.is_active())
            .count())
    }

    pub async fn shutdown(&self) {
        let (tx, rx) = oneshot::channel();
        if self.send(Cmd::Shutdown(tx)).await.is_ok() {
            let _ = tokio::time::timeout(SHUTDOWN_GRACE + Duration::from_secs(1), rx).await;
        }
    }

    /// Subscribes to engine events.
    ///
    /// Events share one bounded broadcast channel. A slow subscriber gets
    /// `RecvError::Lagged(n)` when it falls behind; on `Lagged` it must resync
    /// its view from the store, because `State` events can be dropped along
    /// with `Progress` events.
    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.events.subscribe()
    }

    pub fn store(&self) -> &Store {
        &self.store
    }
}

impl Manager {
    async fn run(mut self, mut rx: mpsc::Receiver<Cmd>) {
        self.schedule().await;
        while let Some(cmd) = rx.recv().await {
            match cmd {
                Cmd::Add(req, reply) => {
                    let r = self.add(req).await;
                    let _ = reply.send(r);
                }
                Cmd::Pause(id) => self.pause(&id).await,
                Cmd::Resume(id) => self.resume(&id).await,
                Cmd::Cancel(id, delete) => self.cancel(&id, delete).await,
                Cmd::Remove(id, delete) => self.remove(&id, delete).await,
                Cmd::PauseAll => {
                    for id in self
                        .ids_in(&[DownloadState::Queued])
                        .await
                        .into_iter()
                        .chain(self.active.keys().cloned().collect::<Vec<_>>())
                    {
                        self.pause(&id).await;
                    }
                }
                Cmd::ResumeAll => {
                    for id in self
                        .ids_in(&[DownloadState::Paused])
                        .await
                        .into_iter()
                        .chain(self.active.keys().cloned().collect::<Vec<_>>())
                    {
                        self.resume(&id).await;
                    }
                }
                Cmd::SetSpeedLimit(bps) => self.env.limiter.set_rate(bps),
                Cmd::SetMaxConcurrent(n) => {
                    self.max_concurrent = n.clamp(1, 10);
                    self.schedule().await;
                }
                Cmd::SetConfig(cfg) => {
                    let cfg = cfg.sanitized();
                    self.env.limiter.set_rate(cfg.speed_limit_bps);
                    self.env.client = build_client(&cfg);
                    self.max_concurrent = cfg.max_concurrent;
                    self.env.config = cfg;
                    self.schedule().await;
                }
                Cmd::Finished(id, outcome) => self.finished(id, outcome).await,
                Cmd::Shutdown(reply) => {
                    self.shutdown_reply = Some(reply);
                    for a in self.active.values_mut() {
                        a.stop = Some(Stop::Shutdown);
                        a.cancel.cancel();
                    }
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(SHUTDOWN_GRACE).await;
                        let _ = tx.send(Cmd::ShutdownDeadline).await;
                    });
                }
                Cmd::ShutdownDeadline => {
                    if let Some(r) = self.shutdown_reply.take() {
                        let _ = r.send(());
                    }
                    break;
                }
            }
            if self.shutdown_reply.is_some() && self.active.is_empty() {
                let _ = self.shutdown_reply.take().unwrap().send(());
                break;
            }
        }
        // rx is dropped here: further handle calls return Err(closed()).
    }

    async fn ids_in(&self, states: &[DownloadState]) -> Vec<String> {
        let mut out = Vec::new();
        for s in states {
            if let Ok(v) = self.env.store.list_by_state(*s).await {
                out.extend(v.into_iter().map(|r| r.id));
            }
        }
        out
    }

    async fn state_of(&self, id: &str) -> Option<DownloadState> {
        self.env
            .store
            .get_download(id)
            .await
            .ok()
            .flatten()
            .map(|r| r.state)
    }

    async fn add(&mut self, req: AddRequest) -> Result<String, OdmError> {
        let scheme_ok = ["http://", "https://"].iter().any(|p| {
            req.url
                .get(..p.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(p))
        });
        if !scheme_ok {
            return Err(OdmError::new(
                ErrorKind::Other,
                "only http(s) URLs are supported",
            ));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let name = req
            .filename
            .as_deref()
            .map(filename::sanitize)
            .or_else(|| filename::from_url(&req.url))
            .unwrap_or_else(|| "download".into());
        let cat = req
            .category
            .clone()
            .unwrap_or_else(|| category::detect(&name, None).as_str().to_string());
        let dir = req
            .save_dir
            .clone()
            .unwrap_or_else(|| self.env.config.save_dir_for(&cat));

        let mut rec = DownloadRecord::new_file(&id, &req.url, &name, &dir.to_string_lossy());
        rec.category = cat;
        rec.auto_place = req.category.is_none() && req.save_dir.is_none();
        rec.filename_locked = req.filename.is_some();
        rec.max_connections = req
            .max_connections
            .unwrap_or(self.env.config.max_connections)
            .clamp(1, 32);
        rec.checksum = req.checksum;
        rec.page_url = req.page_url;
        rec.page_title = req.page_title;
        rec.priority = req.priority;
        if req.start_paused {
            rec.state = DownloadState::Paused;
        }
        self.env
            .store
            .insert_download_with_headers(rec, req.headers)
            .await
            .map_err(store_err)?;
        let _ = self.env.events.send(EngineEvent::Added { id: id.clone() });
        if !req.start_paused {
            self.schedule().await;
        }
        Ok(id)
    }

    fn request_stop(&mut self, id: &str, stop: Stop) -> bool {
        match self.active.get_mut(id) {
            Some(a) => {
                // Remove/Cancel outrank Pause; never downgrade.
                let keep = matches!(a.stop, Some(Stop::Remove { .. } | Stop::Cancel))
                    && stop == Stop::Pause;
                if !keep {
                    a.stop = Some(stop);
                }
                a.cancel.cancel();
                true
            }
            None => false,
        }
    }

    async fn pause(&mut self, id: &str) {
        self.resume_after.remove(id);
        if !self.request_stop(id, Stop::Pause)
            && self.state_of(id).await == Some(DownloadState::Queued)
        {
            transition(&self.env, id, DownloadState::Paused, None).await;
        }
    }

    async fn resume(&mut self, id: &str) {
        if let Some(a) = self.active.get(id) {
            if a.stop == Some(Stop::Pause) {
                self.resume_after.insert(id.to_string());
            }
            return;
        }
        if matches!(
            self.state_of(id).await,
            Some(DownloadState::Paused | DownloadState::Failed)
        ) {
            transition(&self.env, id, DownloadState::Queued, None).await;
            self.schedule().await;
        }
    }

    /// A cancelled download can never be resumed, so its part file always goes.
    /// `_delete_file` only matters for a completed file, which cancel never touches.
    async fn cancel(&mut self, id: &str, _delete_file: bool) {
        self.resume_after.remove(id);
        if self.request_stop(id, Stop::Cancel) {
            return;
        }
        if let Some(s) = self.state_of(id).await {
            if !s.is_terminal() || s == DownloadState::Failed {
                self.delete_part(id).await;
                transition(&self.env, id, DownloadState::Cancelled, None).await;
            }
        }
    }

    async fn remove(&mut self, id: &str, delete: bool) {
        self.resume_after.remove(id);
        if !self.request_stop(id, Stop::Remove { delete }) {
            self.delete_record(id, delete).await;
        }
    }

    async fn delete_part(&self, id: &str) {
        if let Ok(Some(rec)) = self.env.store.get_download(id).await {
            let _ = std::fs::remove_file(part_path(&rec));
        }
    }

    async fn delete_record(&self, id: &str, delete_file: bool) {
        if let Ok(Some(rec)) = self.env.store.get_download(id).await {
            if rec.state == DownloadState::Completed {
                if delete_file {
                    let _ = std::fs::remove_file(Path::new(&rec.save_dir).join(&rec.filename));
                }
            } else {
                // An unfinished download's part file is useless without its record.
                let _ = std::fs::remove_file(part_path(&rec));
            }
        }
        if self.env.store.delete_download(id).await.is_ok() {
            let _ = self
                .env
                .events
                .send(EngineEvent::Removed { id: id.to_string() });
        }
    }

    async fn finished(&mut self, id: String, outcome: Outcome) {
        let stop = self.active.remove(&id).and_then(|a| a.stop);
        match (stop, outcome) {
            (Some(Stop::Remove { delete }), Outcome::Completed { .. }) => {
                transition(&self.env, &id, DownloadState::Completed, None).await;
                self.delete_record(&id, delete).await;
            }
            (Some(Stop::Remove { delete }), _) => self.delete_record(&id, delete).await,
            (_, Outcome::Completed { .. }) => {
                transition(&self.env, &id, DownloadState::Completed, None).await
            }
            (Some(Stop::Pause), _) => transition(&self.env, &id, DownloadState::Paused, None).await,
            (Some(Stop::Cancel), _) => {
                self.delete_part(&id).await;
                transition(&self.env, &id, DownloadState::Cancelled, None).await;
            }
            (Some(Stop::Shutdown), _) => {
                transition(&self.env, &id, DownloadState::Queued, None).await
            }
            (None, Outcome::Failed(e)) => {
                tracing::warn!(%id, "download failed: {e}");
                transition(&self.env, &id, DownloadState::Failed, Some(&e)).await;
            }
            (None, Outcome::Stopped) => {
                transition(&self.env, &id, DownloadState::Paused, None).await
            }
        }
        if self.resume_after.remove(&id) && self.state_of(&id).await == Some(DownloadState::Paused)
        {
            transition(&self.env, &id, DownloadState::Queued, None).await;
        }
        self.schedule().await;
    }

    async fn schedule(&mut self) {
        if self.shutdown_reply.is_some() {
            return;
        }
        let free = self.max_concurrent.saturating_sub(self.active.len());
        if free == 0 {
            return;
        }
        let queued = match self.env.store.list_by_state(DownloadState::Queued).await {
            Ok(q) => q,
            Err(e) => {
                tracing::error!("schedule: {e}");
                return;
            }
        };
        let ids: Vec<String> = queued
            .into_iter()
            .map(|r| r.id)
            .filter(|id| !self.active.contains_key(id))
            .take(free)
            .collect();
        for id in ids {
            self.launch(id);
        }
    }

    fn launch(&mut self, id: String) {
        let cancel = CancellationToken::new();
        let env = self.env.clone();
        let tx = self.tx.clone();
        let (task_id, task_cancel) = (id.clone(), cancel.clone());
        tokio::spawn(async move {
            // Run the download as its own task so a panic surfaces as a JoinError
            // here instead of skipping `Finished` and wedging the id in `active`.
            let inner_id = task_id.clone();
            let out =
                match tokio::spawn(async move { run_download(&env, &inner_id, task_cancel).await })
                    .await
                {
                    Ok(out) => out,
                    Err(e) => {
                        tracing::error!(id = %task_id, "download task panicked: {e}");
                        Outcome::Failed(OdmError::new(ErrorKind::Other, "internal error"))
                    }
                };
            let _ = tx.send(Cmd::Finished(task_id, out)).await;
        });
        self.active.insert(id, Active { cancel, stop: None });
    }
}
