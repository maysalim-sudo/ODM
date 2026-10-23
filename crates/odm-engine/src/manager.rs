use odm_core::{category, filename, DownloadState, ErrorKind, OdmError};
use odm_store::{ChunkRecord, DownloadRecord, Store};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::download::{
    build_client, finalize, part_path, pause_with_reason, run_download, transition, DownloadEnv,
    Outcome, Placed,
};
use crate::hls::run::{run_hls, temp_dir};
use crate::schedule::{Clock, LocalNow, SystemClock};
use crate::site::{run_site, temp_dir as site_temp_dir};
use crate::torrent::run::run_torrent;
use crate::torrent::{
    client_settings, file_records, store_metadata, stored_torrent, write_torrent, TorrentAdded,
    TorrentRequest,
};
use odm_store::{TorrentFileRecord, TorrentMetaRecord};
use odm_torrent::{TorrentClient, TorrentError, TorrentMeta, TorrentSource};
use crate::{EngineConfig, EngineEvent, ExistsChoice, ExistsPolicy, SpeedLimiter};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// `pause_reason` of a download paused by the queue schedule at its stop time.
pub const REASON_SCHEDULE: &str = "schedule";
/// `pause_reason` of a download whose file name is taken, waiting for the user's choice.
pub const REASON_EXISTS: &str = "exists";

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
    /// Start even while the queue schedule is closed ("Start now" in the New download dialog).
    pub start_now: bool,
    /// The size the browser already reported; the probe overwrites it.
    pub size_hint: Option<u64>,
    /// This download's own speed limit in bytes per second; None or 0 = none.
    pub speed_limit: Option<u64>,
}

impl AddRequest {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }
}

/// An HLS stream to download. `playlist_url` may be a master or a media playlist.
#[derive(Debug, Clone, Default)]
pub struct StreamRequest {
    pub playlist_url: String,
    pub variant_url: Option<String>,
    pub filename: Option<String>,
    pub save_dir: Option<PathBuf>,
    pub page_url: Option<String>,
    pub page_title: Option<String>,
    pub headers: Vec<(String, String)>,
    pub start_paused: bool,
    /// Start even while the queue schedule is closed.
    pub start_now: bool,
}

/// A video on a site page (YouTube, Facebook…), downloaded by yt-dlp.
#[derive(Debug, Clone, Default)]
pub struct SiteRequest {
    pub page_url: String,
    pub page_title: Option<String>,
    pub filename: Option<String>,
    pub save_dir: Option<PathBuf>,
    pub headers: Vec<(String, String)>,
    pub start_paused: bool,
    /// Start even while the queue schedule is closed.
    pub start_now: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stop {
    Pause,
    /// The queue schedule's stop time.
    SchedulePause,
    Cancel,
    Remove {
        delete: bool,
    },
    Shutdown,
}

enum Cmd {
    Add(AddRequest, oneshot::Sender<Result<String, OdmError>>),
    AddStream(StreamRequest, oneshot::Sender<Result<String, OdmError>>),
    AddSite(SiteRequest, oneshot::Sender<Result<String, OdmError>>),
    AddTorrent(TorrentRequest, oneshot::Sender<Result<TorrentAdded, OdmError>>),
    /// A torrent's metadata arrived from peers.
    Metadata(String, TorrentMeta),
    SetTorrentFiles(String, Vec<usize>, oneshot::Sender<Result<(), OdmError>>),
    Pause(String),
    Resume(String),
    Cancel(String, bool),
    RefreshUrl(
        String,
        String,
        Option<Vec<(String, String)>>,
        oneshot::Sender<Result<(), OdmError>>,
    ),
    SetTarget(
        String,
        Option<String>,
        Option<PathBuf>,
        oneshot::Sender<Result<(), OdmError>>,
    ),
    Remove(String, bool),
    PauseAll,
    ResumeAll,
    SetSpeedLimit(u64),
    SetMaxConcurrent(usize),
    SetConfig(Box<EngineConfig>),
    Finished(String, Outcome),
    AnswerExists(String, ExistsChoice, oneshot::Sender<Result<(), OdmError>>),
    SetDownloadLimit(String, Option<u64>, oneshot::Sender<Result<(), OdmError>>),
    Tick,
    Shutdown(oneshot::Sender<()>),
    ShutdownDeadline,
}

struct Active {
    cancel: CancellationToken,
    stop: Option<Stop>,
}

/// A finished file whose name is taken, waiting for the user's choice.
struct Staged {
    path: PathBuf,
    name: String,
}

struct Manager {
    env: DownloadEnv,
    tx: mpsc::Sender<Cmd>,
    active: HashMap<String, Active>,
    resume_after: HashSet<String>,
    shutdown_reply: Option<oneshot::Sender<()>>,
    max_concurrent: usize,
    clock: Arc<dyn Clock>,
    /// Downloads paused with reason "exists" in this session, by id.
    waiting: HashMap<String, Staged>,
    /// Whether the queue schedule lets downloads run (always, without one).
    queue_open: bool,
    /// Downloads the user started explicitly (Resume on that row, "Start now"): they run
    /// while the queue schedule is closed. In memory only; cleared when they stop.
    bypass: HashSet<String>,
    /// When the history pass last ran (clock time).
    last_prune: Option<i64>,
    /// A download completed since the queue last reported itself done.
    ran: bool,
    /// Metadata fetches of torrents waiting for their file list, by download id.
    fetchers: HashMap<String, CancellationToken>,
}

#[derive(Clone)]
pub struct ManagerHandle {
    tx: mpsc::Sender<Cmd>,
    events: broadcast::Sender<EngineEvent>,
    store: Store,
    torrents: Arc<dyn TorrentClient>,
}

fn store_err(e: odm_store::StoreError) -> OdmError {
    OdmError::new(ErrorKind::Other, format!("store: {e}"))
}

/// Case-insensitive `http://` / `https://` prefix check.
fn is_http_url(url: &str) -> bool {
    ["http://", "https://"].iter().any(|p| {
        url.get(..p.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(p))
    })
}

/// The folder a stream or site download works in, removed once it is placed.
fn work_dir(rec: &DownloadRecord) -> Option<PathBuf> {
    match rec.kind.as_str() {
        "hls" => Some(temp_dir(rec)),
        "site" => Some(site_temp_dir(rec)),
        _ => None,
    }
}

/// Deletes what an unfinished download leaves behind: its part file, or for an
/// HLS stream its segment directory.
fn remove_partials(rec: &DownloadRecord) {
    let _ = std::fs::remove_file(part_path(rec));
    match rec.kind.as_str() {
        "hls" => {
            let _ = std::fs::remove_dir_all(temp_dir(rec));
        }
        "site" => {
            let _ = std::fs::remove_dir_all(site_temp_dir(rec));
        }
        _ => {}
    }
}

fn closed() -> OdmError {
    OdmError::new(ErrorKind::Other, "engine stopped")
}

impl ManagerHandle {
    pub async fn start(config: EngineConfig, store: Store) -> Result<ManagerHandle, OdmError> {
        Self::start_with_clock(config, store, Arc::new(SystemClock)).await
    }

    /// Like `start`, with this torrent client instead of librqbit.
    pub async fn start_with_torrents(
        config: EngineConfig,
        store: Store,
        torrents: Arc<dyn TorrentClient>,
    ) -> Result<ManagerHandle, OdmError> {
        Self::start_inner(config, store, Arc::new(SystemClock), Some(torrents)).await
    }

    /// Like `start`, reading the schedules and the history age from `clock`.
    pub async fn start_with_clock(
        config: EngineConfig,
        store: Store,
        clock: Arc<dyn Clock>,
    ) -> Result<ManagerHandle, OdmError> {
        Self::start_inner(config, store, clock, None).await
    }

    async fn start_inner(
        config: EngineConfig,
        store: Store,
        clock: Arc<dyn Clock>,
        torrents: Option<Arc<dyn TorrentClient>>,
    ) -> Result<ManagerHandle, OdmError> {
        let config = config.sanitized();
        store.reset_interrupted().await.map_err(store_err)?;
        let (events, _) = broadcast::channel(4096);
        let limiter = Arc::new(SpeedLimiter::new(config.limit_at(clock.now())));
        let mut env = DownloadEnv::new(store.clone(), config.clone(), limiter, events.clone());
        if let Some(t) = torrents {
            env.torrents = t;
        }
        let (tx, rx) = mpsc::channel(256);
        let manager = Manager {
            env,
            tx: tx.clone(),
            active: HashMap::new(),
            resume_after: HashSet::new(),
            shutdown_reply: None,
            max_concurrent: config.max_concurrent.clamp(1, 10),
            clock,
            waiting: HashMap::new(),
            // The first tick opens it (resuming what the schedule paused) unless it is closed.
            queue_open: false,
            bypass: HashSet::new(),
            last_prune: None,
            ran: false,
            fetchers: HashMap::new(),
        };
        let timer = tx.clone();
        let every = config.timer_interval;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(every);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            tick.tick().await;
            loop {
                tick.tick().await;
                if timer.send(Cmd::Tick).await.is_err() {
                    break;
                }
            }
        });
        let torrents = manager.env.torrents.clone();
        tokio::spawn(manager.run(rx));
        Ok(ManagerHandle {
            tx,
            events,
            store,
            torrents,
        })
    }

    async fn send(&self, c: Cmd) -> Result<(), OdmError> {
        self.tx.send(c).await.map_err(|_| closed())
    }

    pub async fn add(&self, req: AddRequest) -> Result<String, OdmError> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::Add(req, tx)).await?;
        rx.await.map_err(|_| closed())?
    }
    pub async fn add_stream(&self, req: StreamRequest) -> Result<String, OdmError> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::AddStream(req, tx)).await?;
        rx.await.map_err(|_| closed())?
    }
    /// Adds the video of a site page; yt-dlp works out the actual media.
    pub async fn add_site(&self, req: SiteRequest) -> Result<String, OdmError> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::AddSite(req, tx)).await?;
        rx.await.map_err(|_| closed())?
    }
    pub async fn add_torrent(&self, req: TorrentRequest) -> Result<TorrentAdded, OdmError> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::AddTorrent(req, tx)).await?;
        rx.await.map_err(|_| closed())?
    }

    /// Chooses which files of a torrent to download; at least one. Running torrents change
    /// at once; a completed one is queued again when a new file still needs downloading.
    pub async fn set_torrent_files(&self, id: &str, selected: Vec<usize>) -> Result<(), OdmError> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::SetTorrentFiles(id.into(), selected, tx)).await?;
        rx.await.map_err(|_| closed())?
    }

    /// Which pieces of a torrent in the session are on disk.
    pub async fn pieces(&self, id: &str) -> Result<Option<odm_torrent::PieceMap>, OdmError> {
        let rec = self.store.get_download(id).await.map_err(store_err)?;
        let hash = rec.and_then(|r| r.info_hash).and_then(|h| odm_torrent::InfoHash::parse(&h));
        Ok(hash.and_then(|h| self.torrents.pieces(&h)))
    }

    /// A torrent's files, by librqbit index. Empty until its metadata is known.
    pub async fn torrent_files(&self, id: &str) -> Result<Vec<TorrentFileRecord>, OdmError> {
        self.store.get_torrent_files(id).await.map_err(store_err)
    }

    pub async fn pause(&self, id: &str) -> Result<(), OdmError> {
        self.send(Cmd::Pause(id.into())).await
    }
    pub async fn resume(&self, id: &str) -> Result<(), OdmError> {
        self.send(Cmd::Resume(id.into())).await
    }
    /// `delete_file` is ignored: cancel always deletes the part file and never touches a completed file.
    pub async fn cancel(&self, id: &str, delete_file: bool) -> Result<(), OdmError> {
        self.send(Cmd::Cancel(id.into(), delete_file)).await
    }
    /// Gives a paused or failed download a new link (and optionally new headers).
    /// It does not resume; the caller resumes.
    pub async fn refresh_url(
        &self,
        id: &str,
        url: String,
        headers: Option<Vec<(String, String)>>,
    ) -> Result<(), OdmError> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::RefreshUrl(id.into(), url, headers, tx))
            .await?;
        rx.await.map_err(|_| closed())?
    }
    /// Change name and/or folder of a download that has not transferred any byte yet (state Paused or
    /// Queued, no chunk with written > 0, not active). The name is sanitized and becomes locked (the
    /// probe won't rename it); a folder turns off auto-placement.
    pub async fn set_target(
        &self,
        id: &str,
        filename: Option<String>,
        save_dir: Option<PathBuf>,
    ) -> Result<(), OdmError> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::SetTarget(id.into(), filename, save_dir, tx))
            .await?;
        rx.await.map_err(|_| closed())?
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

    /// The user's answer to `EngineEvent::ExistsPrompt` for a download paused with reason "exists".
    pub async fn answer_exists(&self, id: &str, choice: ExistsChoice) -> Result<(), OdmError> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::AnswerExists(id.into(), choice, tx)).await?;
        rx.await.map_err(|_| closed())?
    }

    /// A download's own speed limit in bytes per second; None (or 0) removes it. A running
    /// download slows down or speeds up at once.
    pub async fn set_download_limit(&self, id: &str, bps: Option<u64>) -> Result<(), OdmError> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::SetDownloadLimit(id.into(), bps, tx)).await?;
        rx.await.map_err(|_| closed())?
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

    /// Number of downloads per state, read straight from the store.
    pub async fn state_counts(&self) -> Result<HashMap<DownloadState, u64>, OdmError> {
        Ok(self
            .store
            .count_by_state()
            .await
            .map_err(store_err)?
            .into_iter()
            .collect())
    }

    pub async fn active_count(&self) -> Result<usize, OdmError> {
        Ok(self
            .state_counts()
            .await?
            .into_iter()
            .filter(|(s, _)| s.is_active())
            .map(|(_, n)| n as usize)
            .sum())
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
        self.refetch_metadata().await;
        self.tick().await;
        self.schedule().await;
        while let Some(cmd) = rx.recv().await {
            match cmd {
                Cmd::Add(req, reply) => {
                    let r = self.add(req).await;
                    let _ = reply.send(r);
                }
                Cmd::AddSite(req, reply) => {
                    let r = self.add_site(req).await;
                    let _ = reply.send(r);
                }
                Cmd::AddTorrent(req, reply) => {
                    let r = self.add_torrent(req).await;
                    let _ = reply.send(r);
                }
                Cmd::Metadata(id, meta) => self.metadata(id, meta).await,
                Cmd::SetTorrentFiles(id, sel, reply) => {
                    let r = self.set_torrent_files(&id, sel).await;
                    let _ = reply.send(r);
                }
                Cmd::AddStream(req, reply) => {
                    let r = self.add_stream(req).await;
                    let _ = reply.send(r);
                }
                Cmd::Pause(id) => self.pause(&id).await,
                Cmd::Resume(id) => self.resume(&id, true).await,
                Cmd::Cancel(id, delete) => self.cancel(&id, delete).await,
                Cmd::RefreshUrl(id, url, headers, reply) => {
                    let r = self.refresh_url(&id, url, headers).await;
                    let _ = reply.send(r);
                }
                Cmd::SetTarget(id, name, dir, reply) => {
                    let r = self.set_target(&id, name, dir).await;
                    let _ = reply.send(r);
                }
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
                    // What the schedule paused is now paused by the user: it stays paused.
                    for id in self.paused_for(Some(REASON_SCHEDULE)).await {
                        transition(&self.env, &id, DownloadState::Paused, None).await;
                    }
                }
                Cmd::ResumeAll => {
                    // Downloads paused by the schedule or waiting for an answer are left alone.
                    for id in self
                        .paused_for(None)
                        .await
                        .into_iter()
                        .chain(self.active.keys().cloned().collect::<Vec<_>>())
                    {
                        // Resume all follows the queue schedule.
                        self.resume(&id, false).await;
                    }
                }
                Cmd::SetSpeedLimit(bps) => {
                    self.env.config.speed_limit_bps = bps;
                    self.env
                        .limiter
                        .set_rate(self.env.config.limit_at(self.clock.now()));
                    self.apply_torrent_settings().await;
                }
                Cmd::SetMaxConcurrent(n) => {
                    self.max_concurrent = n.clamp(1, 10);
                    self.schedule().await;
                }
                Cmd::SetConfig(cfg) => {
                    let cfg = cfg.sanitized();
                    if cfg.history_days != self.env.config.history_days {
                        self.last_prune = None;
                    }
                    self.env.limiter.set_rate(cfg.limit_at(self.clock.now()));
                    self.env.client = build_client(&cfg);
                    self.max_concurrent = cfg.max_concurrent;
                    self.env.config = cfg;
                    self.apply_torrent_settings().await;
                    self.tick().await;
                    self.schedule().await;
                }
                Cmd::Finished(id, outcome) => self.finished(id, outcome).await,
                Cmd::AnswerExists(id, choice, reply) => {
                    let r = self.answer_exists(&id, choice).await;
                    let _ = reply.send(r);
                }
                Cmd::SetDownloadLimit(id, bps, reply) => {
                    let r = self.set_download_limit(&id, bps).await;
                    let _ = reply.send(r);
                }
                Cmd::Tick => self.tick().await,
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

    /// Paused downloads with this pause reason (None = paused by the user).
    async fn paused_for(&self, reason: Option<&str>) -> Vec<String> {
        match self.env.store.list_by_state(DownloadState::Paused).await {
            Ok(v) => v
                .into_iter()
                .filter(|r| r.pause_reason.as_deref() == reason)
                .map(|r| r.id)
                .collect(),
            Err(e) => {
                tracing::error!("listing paused downloads: {e}");
                Vec::new()
            }
        }
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
        if !is_http_url(&req.url) {
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
        rec.size = req.size_hint.filter(|&n| n > 0);
        rec.speed_limit = req.speed_limit.filter(|&n| n > 0);
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
            if req.start_now {
                self.bypass.insert(id.clone());
            }
            self.schedule().await;
        }
        Ok(id)
    }

    async fn add_stream(&mut self, req: StreamRequest) -> Result<String, OdmError> {
        if !is_http_url(&req.playlist_url) {
            return Err(OdmError::new(
                ErrorKind::Other,
                "only http(s) URLs are supported",
            ));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let name = crate::hls::output_name(
            req.filename.as_deref(),
            req.page_title.as_deref(),
            &req.playlist_url,
        );
        let dir = req
            .save_dir
            .clone()
            .unwrap_or_else(|| self.env.config.save_dir_for("video"));
        let mut rec =
            DownloadRecord::new_hls(&id, &req.playlist_url, &name, &dir.to_string_lossy());
        rec.category = "video".into();
        rec.filename_locked = req.filename.is_some();
        rec.max_connections = self.env.config.max_connections.clamp(1, 32);
        rec.page_url = req.page_url;
        rec.page_title = req.page_title;
        if req.start_paused {
            rec.state = DownloadState::Paused;
        }
        self.env
            .store
            .insert_download_with_headers(rec, req.headers)
            .await
            .map_err(store_err)?;
        self.env
            .store
            .set_hls_meta(
                &id,
                odm_store::HlsMeta {
                    variant_url: req.variant_url.unwrap_or(req.playlist_url),
                    key_url: None,
                    iv: None,
                    map_url: None,
                },
            )
            .await
            .map_err(store_err)?;
        let _ = self.env.events.send(EngineEvent::Added { id: id.clone() });
        if !req.start_paused {
            if req.start_now {
                self.bypass.insert(id.clone());
            }
            self.schedule().await;
        }
        Ok(id)
    }

    async fn add_site(&mut self, req: SiteRequest) -> Result<String, OdmError> {
        if !is_http_url(&req.page_url) {
            return Err(OdmError::new(
                ErrorKind::Other,
                "only http(s) URLs are supported",
            ));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let name = crate::site::output_name(req.filename.as_deref(), req.page_title.as_deref());
        let dir = req
            .save_dir
            .clone()
            .unwrap_or_else(|| self.env.config.save_dir_for("video"));
        let mut rec = DownloadRecord::new_site(&id, &req.page_url, &name, &dir.to_string_lossy());
        rec.category = "video".into();
        rec.filename_locked = req.filename.is_some();
        rec.max_connections = self.env.config.max_connections.clamp(1, 32);
        rec.page_url = Some(req.page_url);
        rec.page_title = req.page_title;
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
            if req.start_now {
                self.bypass.insert(id.clone());
            }
            self.schedule().await;
        }
        Ok(id)
    }

    async fn add_torrent(&mut self, req: TorrentRequest) -> Result<TorrentAdded, OdmError> {
        let parsed = self
            .env
            .torrents
            .parse(&req.source)
            .map_err(|e| OdmError::new(ErrorKind::Other, e.to_string()))?;
        let hash = parsed.info_hash.as_str().to_string();
        let store = &self.env.store;
        if let Some(existing) = store.find_by_info_hash(&hash).await.map_err(store_err)? {
            store
                .merge_trackers(&existing.id, parsed.trackers)
                .await
                .map_err(store_err)?;
            return Ok(TorrentAdded {
                id: existing.id,
                existing: true,
            });
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let name = parsed
            .meta
            .as_ref()
            .map(|m| m.name.clone())
            .or(parsed.name.clone())
            .unwrap_or_else(|| hash.clone());
        let dir = req
            .save_dir
            .clone()
            .unwrap_or_else(|| self.env.config.save_dir_for("torrent"));
        let mut rec = DownloadRecord::new_torrent(
            &id,
            &hash,
            &filename::sanitize(&name),
            &dir.to_string_lossy(),
        );
        rec.state = DownloadState::Paused;
        let source = match &req.source {
            TorrentSource::Magnet(uri) => uri.clone(),
            TorrentSource::Bytes(b) => {
                let path = stored_torrent(&self.env.config, &hash);
                write_torrent(&path, b).map_err(|e| OdmError::new(ErrorKind::Io, e.to_string()))?;
                path.to_string_lossy().into_owned()
            }
        };
        store.insert_download(rec).await.map_err(store_err)?;
        store
            .put_torrent_meta(TorrentMetaRecord {
                download_id: id.clone(),
                source,
                trackers: parsed.trackers,
                has_meta: false,
            })
            .await
            .map_err(store_err)?;
        if let Some(meta) = &parsed.meta {
            store
                .replace_torrent_files(&id, file_records(meta))
                .await
                .map_err(store_err)?;
        }
        let _ = self.env.events.send(EngineEvent::Added { id: id.clone() });
        if parsed.meta.is_none() {
            self.fetch_metadata(&id, req.source);
        }
        Ok(TorrentAdded {
            id,
            existing: false,
        })
    }

    /// Fetches the file list of a magnet that waits for the user's choice. It runs outside
    /// the queue: metadata is small.
    fn fetch_metadata(&mut self, id: &str, source: TorrentSource) {
        let cancel = CancellationToken::new();
        self.fetchers.insert(id.to_string(), cancel.clone());
        let torrents = self.env.torrents.clone();
        let tx = self.tx.clone();
        let id = id.to_string();
        tokio::spawn(async move {
            match torrents.fetch_metadata(&source, cancel).await {
                Ok(meta) => {
                    let _ = tx.send(Cmd::Metadata(id, meta)).await;
                }
                Err(TorrentError::Cancelled) => {}
                Err(e) => tracing::warn!(id = %id, "torrent metadata: {e}"),
            }
        });
    }

    fn stop_fetch(&mut self, id: &str) {
        if let Some(c) = self.fetchers.remove(id) {
            c.cancel();
        }
    }

    /// After a restart, magnets still waiting for their file list fetch it again.
    async fn refetch_metadata(&mut self) {
        let Ok(rows) = self.env.store.list_by_state(DownloadState::Paused).await else {
            return;
        };
        for r in rows.into_iter().filter(|r| r.kind == "torrent") {
            if let Ok(Some(m)) = self.env.store.get_torrent_meta(&r.id).await {
                if !m.has_meta && m.source.starts_with("magnet:") {
                    self.fetch_metadata(&r.id, TorrentSource::Magnet(m.source));
                }
            }
        }
    }

    async fn metadata(&mut self, id: String, meta: TorrentMeta) {
        self.fetchers.remove(&id);
        store_metadata(&self.env, &id, &meta).await;
    }

    async fn set_torrent_files(&mut self, id: &str, selected: Vec<usize>) -> Result<(), OdmError> {
        if selected.is_empty() {
            return Err(OdmError::new(
                ErrorKind::Other,
                "Keep at least one file selected.",
            ));
        }
        let store = self.env.store.clone();
        let rec = store
            .get_download(id)
            .await
            .map_err(store_err)?
            .filter(|r| r.kind == "torrent")
            .ok_or_else(|| OdmError::new(ErrorKind::Other, "not a torrent"))?;
        store
            .set_torrent_selection(id, selected.clone())
            .await
            .map_err(store_err)?;
        if self.active.contains_key(id) {
            if let Some(hash) = rec.info_hash.as_deref().and_then(odm_torrent::InfoHash::parse) {
                if let Err(e) = self.env.torrents.set_files(&hash, &selected).await {
                    tracing::warn!(%id, "changing torrent files: {e}");
                }
            }
        } else if rec.state == DownloadState::Completed {
            let files = store.get_torrent_files(id).await.map_err(store_err)?;
            if files.iter().any(|f| f.selected && f.done < f.size) {
                transition(&self.env, id, DownloadState::Queued, None).await;
                self.schedule().await;
            }
        }
        // The UI reloads the file list on this event.
        let _ = self.env.events.send(EngineEvent::TorrentMetadata { id: id.to_string() });
        Ok(())
    }

    async fn apply_torrent_settings(&self) {
        let rate = self.env.config.limit_at(self.clock.now());
        let s = client_settings(&self.env.config, rate);
        if let Err(e) = self.env.torrents.apply_settings(s).await {
            tracing::warn!("torrent settings: {e}");
        }
    }

    async fn refresh_url(
        &mut self,
        id: &str,
        url: String,
        headers: Option<Vec<(String, String)>>,
    ) -> Result<(), OdmError> {
        if !is_http_url(&url) {
            return Err(OdmError::new(
                ErrorKind::Other,
                "only http(s) URLs are supported",
            ));
        }
        if !matches!(
            self.state_of(id).await,
            Some(DownloadState::Paused | DownloadState::Failed)
        ) || self.active.contains_key(id)
        {
            return Err(OdmError::new(
                ErrorKind::Other,
                "only a paused or failed download can get a new link",
            ));
        }
        self.env.store.set_url(id, url).await.map_err(store_err)?;
        if let Some(h) = headers {
            self.env.store.set_headers(id, h).await.map_err(store_err)?;
        }
        Ok(())
    }

    async fn set_target(
        &mut self,
        id: &str,
        filename: Option<String>,
        save_dir: Option<PathBuf>,
    ) -> Result<(), OdmError> {
        let started = || OdmError::new(ErrorKind::Other, "this download has already started");
        if self.active.contains_key(id)
            || !matches!(
                self.state_of(id).await,
                Some(DownloadState::Paused | DownloadState::Queued)
            )
        {
            return Err(started());
        }
        let written: u64 = self
            .env
            .store
            .get_chunks(id)
            .await
            .map_err(store_err)?
            .iter()
            .map(|c| c.written)
            .sum();
        if written > 0 {
            return Err(started());
        }
        let name = filename
            .map(|n| filename::sanitize(&n))
            .filter(|n| !n.is_empty());
        let dir = save_dir.map(|d| d.to_string_lossy().into_owned());
        self.env
            .store
            .set_target(id, name, dir)
            .await
            .map_err(store_err)
    }

    fn request_stop(&mut self, id: &str, stop: Stop) -> bool {
        match self.active.get_mut(id) {
            Some(a) => {
                // Remove/Cancel outrank a pause; never downgrade.
                let keep = matches!(a.stop, Some(Stop::Remove { .. } | Stop::Cancel))
                    && matches!(stop, Stop::Pause | Stop::SchedulePause);
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
        self.bypass.remove(id);
        if !self.request_stop(id, Stop::Pause)
            && self.state_of(id).await == Some(DownloadState::Queued)
        {
            transition(&self.env, id, DownloadState::Paused, None).await;
        }
    }

    /// `explicit`: the user resumed this download itself, so it runs even while the queue
    /// schedule is closed. Resume all is not explicit.
    async fn resume(&mut self, id: &str, explicit: bool) {
        if let Some(staged) = self.waiting.get(id) {
            // The file is ready; only the answer is missing. Ask again.
            let _ = self.env.events.send(EngineEvent::ExistsPrompt {
                id: id.to_string(),
                name: staged.name.clone(),
            });
            return;
        }
        if let Some(a) = self.active.get(id) {
            if a.stop == Some(Stop::Pause) {
                self.resume_after.insert(id.to_string());
                if explicit {
                    self.bypass.insert(id.to_string());
                }
            }
            return;
        }
        match self.state_of(id).await {
            Some(DownloadState::Paused | DownloadState::Failed) => {
                if explicit {
                    self.bypass.insert(id.to_string());
                }
                transition(&self.env, id, DownloadState::Queued, None).await;
                self.schedule().await;
            }
            // Waiting for the queue schedule: Resume on the row starts it now.
            Some(DownloadState::Queued) if explicit => {
                self.bypass.insert(id.to_string());
                self.schedule().await;
            }
            _ => {}
        }
    }

    /// A cancelled download can never be resumed, so its part file always goes.
    /// `_delete_file` only matters for a completed file, which cancel never touches.
    async fn cancel(&mut self, id: &str, _delete_file: bool) {
        self.stop_fetch(id);
        self.resume_after.remove(id);
        self.bypass.remove(id);
        self.drop_staged(id);
        if self.request_stop(id, Stop::Cancel) {
            return;
        }
        if let Some(s) = self.state_of(id).await {
            if !s.is_terminal() || s == DownloadState::Failed {
                self.delete_part(id).await;
                transition(&self.env, id, DownloadState::Cancelled, None).await;
            }
        }
        // It may have been the last thing left (a download waiting for an answer).
        self.check_queue_done().await;
    }

    async fn remove(&mut self, id: &str, delete: bool) {
        self.stop_fetch(id);
        self.resume_after.remove(id);
        self.bypass.remove(id);
        self.drop_staged(id);
        if !self.request_stop(id, Stop::Remove { delete }) {
            self.delete_record(id, delete).await;
            self.check_queue_done().await;
        }
    }

    async fn delete_part(&self, id: &str) {
        if let Ok(Some(rec)) = self.env.store.get_download(id).await {
            if rec.kind == "torrent" {
                // A cancelled torrent cannot be resumed: its files go.
                self.drop_torrent(&rec, true).await;
            } else {
                remove_partials(&rec);
            }
        }
    }

    /// Takes a torrent out of the session, deleting its files when asked.
    async fn drop_torrent(&self, rec: &DownloadRecord, delete_files: bool) {
        let Some(hash) = rec.info_hash.as_deref().and_then(odm_torrent::InfoHash::parse) else {
            return;
        };
        if let Err(e) = self.env.torrents.remove(&hash, delete_files).await {
            tracing::warn!(id = %rec.id, "removing torrent: {e}");
        }
        if delete_files {
            // Not in the session (completed, or never started): delete what is on disk.
            if let Ok(files) = self.env.store.get_torrent_files(&rec.id).await {
                let out = crate::torrent::run::output_dir(rec, &files);
                for f in &files {
                    let _ = std::fs::remove_file(out.join(&f.path));
                }
                if files.len() > 1 {
                    let _ = std::fs::remove_dir_all(&out).or_else(|_| std::fs::remove_dir(&out));
                }
            }
        }
    }

    async fn delete_record(&self, id: &str, delete_file: bool) {
        if let Ok(Some(rec)) = self.env.store.get_download(id).await {
            if rec.kind == "torrent" {
                self.drop_torrent(&rec, delete_file).await;
                if let Some(hash) = &rec.info_hash {
                    let _ = std::fs::remove_file(stored_torrent(&self.env.config, hash));
                }
            } else if rec.state == DownloadState::Completed {
                if delete_file {
                    let _ = std::fs::remove_file(Path::new(&rec.save_dir).join(&rec.filename));
                }
            } else {
                // An unfinished download's part file is useless without its record.
                remove_partials(&rec);
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
        if !self.resume_after.contains(&id) {
            self.bypass.remove(&id);
        }
        // Paused, by the user or by the schedule's stop time: the work is not done.
        let paused = match (&stop, &outcome) {
            (_, Outcome::Completed { .. } | Outcome::Exists { .. }) => false,
            (Some(Stop::Pause | Stop::SchedulePause), _) | (None, Outcome::Stopped) => true,
            _ => false,
        };
        match (stop, outcome) {
            (Some(Stop::Remove { delete }), Outcome::Completed { .. }) => {
                transition(&self.env, &id, DownloadState::Completed, None).await;
                self.delete_record(&id, delete).await;
            }
            (Some(Stop::Remove { delete }), Outcome::Exists { staged, .. }) => {
                let _ = std::fs::remove_file(staged);
                self.delete_record(&id, delete).await;
            }
            (Some(Stop::Remove { delete }), _) => self.delete_record(&id, delete).await,
            (Some(Stop::Cancel), Outcome::Exists { staged, .. }) => {
                let _ = std::fs::remove_file(staged);
                self.delete_part(&id).await;
                transition(&self.env, &id, DownloadState::Cancelled, None).await;
            }
            (_, Outcome::Completed { .. }) => {
                self.ran = true;
                transition(&self.env, &id, DownloadState::Completed, None).await
            }
            (_, Outcome::Exists { staged, name }) => self.wait_for_answer(&id, staged, name).await,
            (Some(Stop::Pause), _) => transition(&self.env, &id, DownloadState::Paused, None).await,
            (Some(Stop::SchedulePause), _) => {
                pause_with_reason(&self.env, &id, REASON_SCHEDULE).await
            }
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
        if self.resume_after.remove(&id)
            && !self.waiting.contains_key(&id)
            && self.state_of(&id).await == Some(DownloadState::Paused)
        {
            transition(&self.env, &id, DownloadState::Queued, None).await;
        }
        self.schedule().await;
        if !paused {
            self.check_queue_done().await;
        }
    }

    /// Pauses a finished download whose name is taken and asks the user what to do.
    async fn wait_for_answer(&mut self, id: &str, staged: PathBuf, name: String) {
        pause_with_reason(&self.env, id, REASON_EXISTS).await;
        self.waiting.insert(
            id.to_string(),
            Staged {
                path: staged,
                name: name.clone(),
            },
        );
        let _ = self.env.events.send(EngineEvent::ExistsPrompt {
            id: id.to_string(),
            name,
        });
    }

    /// Forgets a waiting download's finished file and deletes it.
    fn drop_staged(&mut self, id: &str) {
        if let Some(s) = self.waiting.remove(id) {
            let _ = std::fs::remove_file(s.path);
        }
    }

    async fn answer_exists(&mut self, id: &str, choice: ExistsChoice) -> Result<(), OdmError> {
        let policy = match choice {
            ExistsChoice::Replace => Some(ExistsPolicy::Overwrite),
            ExistsChoice::KeepBoth => Some(ExistsPolicy::Rename),
            ExistsChoice::Cancel => None,
        };
        let not_waiting = || {
            OdmError::new(
                ErrorKind::Other,
                "this download is not waiting for an answer",
            )
        };
        let Some(staged) = self.waiting.remove(id) else {
            // Asked in an earlier session: the finished file is not known any more.
            // Download it again with the answer, or cancel it.
            let rec = self
                .env
                .store
                .get_download(id)
                .await
                .map_err(store_err)?
                .ok_or_else(not_waiting)?;
            if rec.state != DownloadState::Paused
                || rec.pause_reason.as_deref() != Some(REASON_EXISTS)
            {
                return Err(not_waiting());
            }
            match policy {
                Some(p) => {
                    self.set_override(id, p);
                    transition(&self.env, id, DownloadState::Queued, None).await;
                    self.schedule().await;
                }
                None => self.cancel(id, false).await,
            }
            return Ok(());
        };
        let Some(policy) = policy else {
            let _ = std::fs::remove_file(&staged.path);
            self.delete_part(id).await;
            transition(&self.env, id, DownloadState::Cancelled, None).await;
            self.check_queue_done().await;
            return Ok(());
        };
        self.set_override(id, policy);
        let rec = self
            .env
            .store
            .get_download(id)
            .await
            .map_err(store_err)?
            .ok_or_else(not_waiting)?;
        match finalize(&self.env, &rec, &staged.path, &staged.name, work_dir(&rec)).await {
            Ok(Placed::Done(_)) => {
                self.ran = true;
                transition(&self.env, id, DownloadState::Completed, None).await;
            }
            Ok(Placed::Exists { staged, name }) => self.wait_for_answer(id, staged, name).await,
            Err(e) => {
                tracing::warn!(%id, "could not place the file: {e}");
                transition(&self.env, id, DownloadState::Failed, Some(&e)).await;
            }
        }
        self.schedule().await;
        self.check_queue_done().await;
        Ok(())
    }

    fn set_override(&self, id: &str, policy: ExistsPolicy) {
        self.env
            .exists_overrides
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id.to_string(), policy);
    }

    async fn set_download_limit(&mut self, id: &str, bps: Option<u64>) -> Result<(), OdmError> {
        let bps = bps.filter(|&n| n > 0);
        if self.state_of(id).await.is_none() {
            return Err(OdmError::new(ErrorKind::Other, "download not found"));
        }
        self.env
            .store
            .set_speed_limit(id, bps)
            .await
            .map_err(store_err)?;
        if let Some(l) = self
            .env
            .local_limits
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)
        {
            l.set_rate(bps.unwrap_or(0));
        }
        Ok(())
    }

    /// Sends `QueueDone` once the queue has run something and has nothing left: nothing
    /// queued, running, paused by the schedule or waiting for an answer.
    async fn check_queue_done(&mut self) {
        if !self.ran
            || self.shutdown_reply.is_some()
            || !self.active.is_empty()
            || !self.waiting.is_empty()
        {
            return;
        }
        let left = async {
            let queued = self.env.store.list_by_state(DownloadState::Queued).await?;
            let paused = self.env.store.list_by_state(DownloadState::Paused).await?;
            Ok::<bool, odm_store::StoreError>(
                !queued.is_empty()
                    || paused.iter().any(|r| {
                        matches!(
                            r.pause_reason.as_deref(),
                            Some(REASON_SCHEDULE | REASON_EXISTS)
                        )
                    }),
            )
        };
        match left.await {
            Ok(false) => {
                self.ran = false;
                let _ = self.env.events.send(EngineEvent::QueueDone);
            }
            Ok(true) => {}
            Err(e) => tracing::error!("queue check: {e}"),
        }
    }

    /// The timer: the speed-limit and queue schedules, and the daily history pass.
    async fn tick(&mut self) {
        if self.shutdown_reply.is_some() {
            return;
        }
        let now = self.clock.now();
        let rate = self.env.config.limit_at(now);
        if self.env.limiter.rate() != rate {
            self.env.limiter.set_rate(rate);
            self.apply_torrent_settings().await;
        }
        let open = self.env.config.queue_open_at(now);
        if open != self.queue_open {
            self.queue_open = open;
            if open {
                for id in self.paused_for(Some(REASON_SCHEDULE)).await {
                    transition(&self.env, &id, DownloadState::Queued, None).await;
                }
                self.schedule().await;
            } else {
                // What the user started explicitly keeps running.
                let ids: Vec<String> = self
                    .active
                    .keys()
                    .filter(|id| !self.bypass.contains(*id))
                    .cloned()
                    .collect();
                for id in ids {
                    self.resume_after.remove(&id);
                    self.request_stop(&id, Stop::SchedulePause);
                }
            }
        }
        self.prune_history(now).await;
    }

    /// Once a day, deletes the records (never the files) of downloads completed more
    /// than `history_days` ago.
    async fn prune_history(&mut self, now: LocalNow) {
        let days = self.env.config.history_days;
        if days == 0 {
            return;
        }
        if self
            .last_prune
            .is_some_and(|t| now.unix_ms >= t && now.unix_ms - t < DAY_MS)
        {
            return;
        }
        self.last_prune = Some(now.unix_ms);
        let cutoff = now.unix_ms - i64::from(days) * DAY_MS;
        match self.env.store.delete_completed_before(cutoff).await {
            Ok(ids) => {
                if !ids.is_empty() {
                    tracing::info!(
                        count = ids.len(),
                        "removed old finished downloads from the list"
                    );
                }
                for id in ids {
                    let _ = self.env.events.send(EngineEvent::Removed { id });
                }
            }
            Err(e) => tracing::error!("history pass: {e}"),
        }
    }

    async fn schedule(&mut self) {
        if self.shutdown_reply.is_some() || (!self.queue_open && self.bypass.is_empty()) {
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
        // While the queue schedule is closed, only what the user started explicitly runs.
        let ids: Vec<String> = queued
            .into_iter()
            .map(|r| r.id)
            .filter(|id| !self.active.contains_key(id))
            .filter(|id| self.queue_open || self.bypass.contains(id))
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
            let out = match tokio::spawn(async move {
                let kind = match env.store.get_download(&inner_id).await {
                    Ok(Some(r)) => r.kind,
                    _ => String::new(),
                };
                match kind.as_str() {
                    "hls" => run_hls(&env, &inner_id, task_cancel).await,
                    "site" => run_site(&env, &inner_id, task_cancel).await,
                    "torrent" => run_torrent(&env, &inner_id, task_cancel).await,
                    _ => run_download(&env, &inner_id, task_cancel).await,
                }
            })
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
