//! [`TorrentClient`] on a librqbit session. The session starts on first use; restored
//! torrents are paused so only the engine's queue starts them.
use crate::port::pick_port;
use bytes::Bytes;
use crate::*;
use librqbit::api::TorrentIdOrHash;
use librqbit::dht::Id20;
use librqbit::limits::LimitsConfig;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, Api, ConnectionOptions, DhtSessionConfig,
    ListenerOptions, Magnet, ManagedTorrent, Session, SessionOptions, SessionPersistenceConfig,
    TorrentStatsState, ValidatedTorrentMetaV1Info,
};
use std::collections::HashSet;
use std::num::NonZeroU32;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

pub struct LibrqbitClient {
    settings: Mutex<TorrentSettings>,
    /// Serializes session creation and restarts.
    init: tokio::sync::Mutex<()>,
    session: RwLock<Option<Arc<Session>>>,
    /// Torrents this client added (as opposed to ones restored from an earlier run).
    added: Mutex<HashSet<InfoHash>>,
    /// Metadata fetches running now: they keep the session busy like a running torrent.
    fetching: AtomicUsize,
    /// Settings changed while busy: restart the session once it is idle.
    restart_pending: AtomicBool,
    /// Torrents the engine has started and not paused or removed since. Anything else in
    /// the session is held paused, including torrents librqbit restores as running.
    wanted: Arc<Mutex<HashSet<InfoHash>>>,
}

fn session_err(e: impl std::fmt::Display) -> TorrentError {
    TorrentError::Session(e.to_string())
}

fn id20(h: &InfoHash) -> Id20 {
    // InfoHash is always 40 hex characters.
    Id20::from_str(h.as_str()).expect("valid info hash")
}

fn nz(bps: Option<u64>) -> Option<NonZeroU32> {
    bps.and_then(|b| NonZeroU32::new(b.min(u32::MAX as u64) as u32))
}

fn meta_from(info: &ValidatedTorrentMetaV1Info<librqbit::ByteBufOwned>, bytes: Bytes) -> TorrentMeta {
    let files = info
        .iter_file_details()
        .enumerate()
        .filter(|(_, d)| !d.attrs().padding)
        .map(|(idx, d)| TorrentFile {
            idx,
            path: d.filename.to_vec().join("/"),
            size: d.len,
        })
        .collect();
    TorrentMeta {
        name: info.name().map(|n| n.into_owned()).unwrap_or_else(|| "torrent".into()),
        files,
        torrent_bytes: bytes,
    }
}

/// Settings that only take effect when a session is created.
fn session_key(s: &TorrentSettings) -> impl PartialEq {
    (
        s.listen_port,
        s.upnp,
        s.socks_proxy.clone(),
        s.upload_bps == Some(0),
        s.offline,
        s.state_dir.clone(),
    )
}

impl LibrqbitClient {
    pub fn new(settings: TorrentSettings) -> Arc<Self> {
        Arc::new(Self {
            settings: Mutex::new(settings),
            init: tokio::sync::Mutex::new(()),
            session: RwLock::new(None),
            added: Mutex::new(HashSet::new()),
            fetching: AtomicUsize::new(0),
            restart_pending: AtomicBool::new(false),
            wanted: Arc::default(),
        })
    }

    fn settings(&self) -> TorrentSettings {
        self.settings.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn added(&self) -> std::sync::MutexGuard<'_, HashSet<InfoHash>> {
        self.added.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn current(&self) -> Option<Arc<Session>> {
        self.session.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn handle(&self, h: &InfoHash) -> Option<Arc<ManagedTorrent>> {
        self.current()?.get(TorrentIdOrHash::Hash(id20(h)))
    }

    /// Starts the session now, loading torrents saved by an earlier run (paused).
    pub async fn restore(&self) -> Result<(), TorrentError> {
        self.session().await.map(|_| ())
    }

    /// Pauses everything so the saved state restores paused, then stops the session.
    /// Stops the session the way a crash would: nothing is paused first, so the saved
    /// session still says "running". Tests only.
    #[doc(hidden)]
    pub async fn crash_for_tests(&self) {
        let _guard = self.init.lock().await;
        let taken = self.session.write().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(s) = taken {
            s.stop().await;
        }
    }

    /// Whether librqbit is transferring this torrent (not paused, not checking). Tests only.
    #[doc(hidden)]
    pub fn running_for_tests(&self, hash: &InfoHash) -> bool {
        self.handle(hash).is_some_and(|h| {
            h.with_state(|s| matches!(s, librqbit::ManagedTorrentState::Live(_)))
        })
    }

    pub async fn shutdown(&self) {
        let _guard = self.init.lock().await;
        let taken = self.session.write().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(s) = taken {
            pause_all(&s).await;
            s.stop().await;
        }
    }

    pub fn active_port(&self) -> Option<u16> {
        self.current()?.listen_addr().map(|a| a.port())
    }

    async fn session(&self) -> Result<Arc<Session>, TorrentError> {
        if let Some(s) = self.current() {
            return Ok(s);
        }
        let _guard = self.init.lock().await;
        if let Some(s) = self.current() {
            return Ok(s);
        }
        let s = self.create().await?;
        pause_all(&s).await;
        self.hold_restored(&s);
        *self.session.write().unwrap_or_else(|e| e.into_inner()) = Some(s.clone());
        Ok(s)
    }

    /// librqbit ignores a pause while it checks a restored torrent's data, then runs it if the
    /// saved session said so (after a crash). Pause each one once its check ends, unless the
    /// engine has started it by then.
    fn hold_restored(&self, s: &Arc<Session>) {
        let handles: Vec<_> = s.with_torrents(|it| it.map(|(_, h)| h.clone()).collect());
        for h in handles {
            let (s, wanted) = (s.clone(), self.wanted.clone());
            tokio::spawn(async move {
                if h.wait_until_initialized().await.is_err() {
                    return;
                }
                let hash = InfoHash::parse(&h.info_hash().as_string());
                let keep = hash.is_some_and(|hash| {
                    wanted.lock().unwrap_or_else(|e| e.into_inner()).contains(&hash)
                });
                if !keep && is_live(&h) {
                    let _ = s.pause(&h).await;
                }
            });
        }
    }

    fn want(&self, hash: &InfoHash, on: bool) {
        let mut w = self.wanted.lock().unwrap_or_else(|e| e.into_inner());
        if on {
            w.insert(hash.clone());
        } else {
            w.remove(hash);
        }
    }

    async fn create(&self) -> Result<Arc<Session>, TorrentError> {
        let set = self.settings();
        std::fs::create_dir_all(&set.state_dir).map_err(|e| TorrentError::Io(e.to_string()))?;
        let port = pick_port(set.listen_port)
            .ok_or_else(|| TorrentError::Session("no free port for torrents".into()))?;
        let opts = SessionOptions {
            dht: (!set.offline).then(|| DhtSessionConfig {
                persistence: Some(librqbit::dht::DhtPersistenceConfig {
                    config_filename: Some(set.state_dir.join("dht.json")),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            disable_trackers: set.offline,
            disable_local_service_discovery: set.offline,
            fastresume: true,
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(set.state_dir.join("session")),
            }),
            listen: Some(ListenerOptions {
                listen_addr: ([0, 0, 0, 0], port).into(),
                enable_upnp_port_forwarding: set.upnp && !set.offline,
                ipv4_only: true,
                ..Default::default()
            }),
            connect: Some(ConnectionOptions {
                proxy_url: set.socks_proxy.clone(),
                ..Default::default()
            }),
            ratelimits: LimitsConfig {
                upload_bps: nz(set.upload_bps),
                download_bps: nz(set.download_bps),
            },
            peer_limit: Some(set.peer_limit),
            disable_upload: set.upload_bps == Some(0),
            ipv4_only: true,
            ..Default::default()
        };
        Session::new_with_opts(set.state_dir.join("default"), opts)
            .await
            .map_err(session_err)
    }

    /// True while a torrent transfers or a metadata fetch runs.
    fn busy(&self) -> bool {
        if self.fetching.load(Ordering::SeqCst) > 0 {
            return true;
        }
        let Some(session) = self.current() else {
            return false;
        };
        session.with_torrents(|it| {
            for (_, h) in it {
                if is_live(h) {
                    return true;
                }
            }
            false
        })
    }

    /// Restarts the session for changed settings once nothing uses it. The next call
    /// that needs it starts it with the new settings.
    async fn restart_if_idle(&self) {
        if self.restart_pending.load(Ordering::SeqCst) && !self.busy() {
            self.restart_pending.store(false, Ordering::SeqCst);
            self.shutdown().await;
        }
    }

    fn initial_peers(&self) -> Option<Vec<std::net::SocketAddr>> {
        let p = self.settings().extra_peers;
        (!p.is_empty()).then_some(p)
    }
}

/// True when `trackers` or `peers` hold something the torrent in the session was added without.
/// `added_here` says whether this client added it (a restored torrent never got `peers`).
fn needs_readd(
    h: &ManagedTorrent,
    save_dir: &std::path::Path,
    trackers: &[String],
    peers: &[std::net::SocketAddr],
    added_here: bool,
) -> bool {
    // A torrent restored from an earlier run keeps its old folder unless it is added again.
    if h.output_folder() != save_dir {
        return true;
    }
    let shared = h.shared();
    let known: HashSet<String> = shared.trackers.iter().map(|u| u.to_string()).collect();
    let new_tracker = trackers.iter().any(|t| {
        let norm = url::Url::parse(t).map(|u| u.to_string()).unwrap_or_else(|_| t.clone());
        !known.contains(&norm)
    });
    let new_peer = !peers.is_empty() && !added_here;
    new_tracker || new_peer
}

/// Creates the torrent's missing files as sparse files before librqbit opens them, so a file
/// that is not downloaded (or not yet) takes no disk space. librqbit sizes every file to its
/// full length and its own sparse marking does not take on Windows here.
fn create_sparse(out: &std::path::Path, files: &[TorrentFile]) {
    for f in files {
        let mut p = out.to_path_buf();
        for part in f.path.split('/') {
            p.push(part);
        }
        if p.exists() {
            continue;
        }
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(file) = std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
            mark_sparse(&file);
        }
    }
}

#[cfg(windows)]
fn mark_sparse(f: &std::fs::File) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;
    use windows_sys::Win32::System::IO::DeviceIoControl;
    let mut returned = 0u32;
    // SAFETY: a valid handle of an open file; no input or output buffers.
    unsafe {
        DeviceIoControl(
            f.as_raw_handle() as _,
            FSCTL_SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        );
    }
}

#[cfg(not(windows))]
fn mark_sparse(_f: &std::fs::File) {}

fn is_live(h: &ManagedTorrent) -> bool {
    h.with_state(|st| matches!(st, librqbit::ManagedTorrentState::Live(_)))
}

fn is_checking(h: &ManagedTorrent) -> bool {
    h.with_state(|st| matches!(st, librqbit::ManagedTorrentState::Initializing(_)))
}

async fn pause_all(s: &Arc<Session>) {
    let live: Vec<_> = s.with_torrents(|it| {
        it.filter(|(_, h)| is_live(h) || is_checking(h))
            .map(|(_, h)| h.clone())
            .collect()
    });
    for h in live {
        let _ = s.pause(&h).await;
    }
}

#[async_trait]
impl TorrentClient for LibrqbitClient {
    async fn fetch_metadata(
        &self,
        source: &TorrentSource,
        cancel: CancellationToken,
    ) -> Result<TorrentMeta, TorrentError> {
        let uri = match source {
            TorrentSource::Bytes(_) => {
                return parse(source)?.meta.ok_or_else(|| TorrentError::Invalid("no metadata".into()))
            }
            TorrentSource::Magnet(u) => u.clone(),
        };
        let session = self.session().await?;
        self.fetching.fetch_add(1, Ordering::SeqCst);
        let result = async {
            let opts = AddTorrentOptions {
                list_only: true,
                initial_peers: self.initial_peers(),
                ..Default::default()
            };
            let add = session.add_torrent(AddTorrent::from_url(uri), Some(opts));
            let resp = tokio::select! {
                _ = cancel.cancelled() => return Err(TorrentError::Cancelled),
                r = add => r.map_err(session_err)?,
            };
            let meta = match resp {
                AddTorrentResponse::ListOnly(r) => meta_from(&r.info, r.torrent_bytes),
                AddTorrentResponse::AlreadyManaged(_, h) | AddTorrentResponse::Added(_, h) => h
                    .with_metadata(|m| meta_from(&m.info, m.torrent_bytes.clone()))
                    .map_err(session_err)?,
            };
            // A magnet's names are only known now; refuse the ones Windows cannot use.
            check_names(&meta.files)?;
            Ok(meta)
        }
        .await;
        self.fetching.fetch_sub(1, Ordering::SeqCst);
        self.restart_if_idle().await;
        result
    }

    async fn start(&self, req: StartTorrent) -> Result<(), TorrentError> {
        let session = self.session().await?;
        if let Some(h) = session.get(TorrentIdOrHash::Hash(id20(&req.info_hash))) {
            let added_here = self.added().contains(&req.info_hash);
            if !needs_readd(&h, &req.save_dir, &req.trackers, &self.settings().extra_peers, added_here) {
                self.want(&req.info_hash, true);
                // Restored torrents check their data first; librqbit refuses changes until then.
                h.wait_until_initialized().await.map_err(session_err)?;
                if let Some(only) = &req.only_files {
                    let set: HashSet<usize> = only.iter().copied().collect();
                    session.update_only_files(&h, &set).await.map_err(session_err)?;
                }
                // After a crash librqbit may already run it; unpausing a live torrent fails.
                if !is_live(&h) {
                    session.unpause(&h).await.map_err(session_err)?;
                }
                return Ok(());
            }
            // New trackers or peers only reach librqbit when the torrent is added; the data
            // on disk is kept and checked again, not downloaded again.
            session
                .delete(TorrentIdOrHash::Hash(id20(&req.info_hash)), false)
                .await
                .map_err(session_err)?;
        }
        let add = match &req.source {
            TorrentSource::Bytes(b) => AddTorrent::from_bytes(with_trackers(b, &req.trackers)?),
            TorrentSource::Magnet(uri) => {
                let parsed = parse(&req.source)?;
                let mut trackers = parsed.trackers;
                for t in &req.trackers {
                    if !trackers.contains(t) {
                        trackers.push(t.clone());
                    }
                }
                let _ = uri;
                AddTorrent::from_url(Magnet::from_id20(id20(&req.info_hash), trackers, None).to_string())
            }
        };
        let set = self.settings();
        let opts = AddTorrentOptions {
            paused: false,
            only_files: req.only_files.clone(),
            output_folder: Some(req.save_dir.to_string_lossy().into_owned()),
            overwrite: true,
            initial_peers: self.initial_peers(),
            peer_limit: Some(set.peer_limit),
            ratelimits: LimitsConfig {
                upload_bps: None,
                download_bps: nz(req.download_bps),
            },
            ..Default::default()
        };
        if let TorrentSource::Bytes(_) = &req.source {
            if let Ok(Some(meta)) = parse(&req.source).map(|p| p.meta) {
                create_sparse(&req.save_dir, &meta.files);
            }
        }
        session.add_torrent(add, Some(opts)).await.map_err(session_err)?;
        self.want(&req.info_hash, true);
        self.added().insert(req.info_hash);
        Ok(())
    }

    async fn set_files(&self, hash: &InfoHash, selected: &[usize]) -> Result<(), TorrentError> {
        let session = self.current().ok_or(TorrentError::NotFound)?;
        let h = self.handle(hash).ok_or(TorrentError::NotFound)?;
        // librqbit refuses changes while it checks existing data.
        h.wait_until_initialized().await.map_err(session_err)?;
        let set: HashSet<usize> = selected.iter().copied().collect();
        session.update_only_files(&h, &set).await.map_err(session_err)
    }

    async fn pause(&self, hash: &InfoHash) -> Result<(), TorrentError> {
        self.want(hash, false);
        let (Some(session), Some(h)) = (self.current(), self.handle(hash)) else {
            return Ok(());
        };
        // librqbit's paused flag can be set on a torrent that went live anyway (paused while
        // it checked its data), so go by its state.
        if !is_live(&h) && !is_checking(&h) {
            return Ok(());
        }
        session.pause(&h).await.map_err(session_err)?;
        self.restart_if_idle().await;
        Ok(())
    }

    async fn remove(&self, hash: &InfoHash, delete_files: bool) -> Result<(), TorrentError> {
        // Start the session if needed: a torrent saved by an earlier run must go too, or it
        // comes back with every later session.
        let session = self.session().await?;
        if self.handle(hash).is_none() {
            return Ok(());
        }
        self.added().remove(hash);
        self.want(hash, false);
        session
            .delete(TorrentIdOrHash::Hash(id20(hash)), delete_files)
            .await
            .map_err(session_err)?;
        self.restart_if_idle().await;
        Ok(())
    }

    fn stats(&self, hash: &InfoHash) -> Option<TorrentStats> {
        let h = self.handle(hash)?;
        let st = h.stats();
        // (size, is padding) per librqbit file index.
        let sizes: Option<Vec<(u64, bool)>> = h
            .with_metadata(|m| {
                m.info
                    .iter_file_details()
                    .map(|d| (d.len, d.attrs().padding))
                    .collect()
            })
            .ok();
        let has_meta = sizes.is_some();
        let only = h.only_files();
        let selected = |idx: usize| only.as_ref().is_none_or(|o| o.contains(&idx));
        let (mut sel_total, mut sel_done) = (0u64, 0u64);
        for (idx, &(len, pad)) in sizes.iter().flatten().enumerate() {
            if pad || !selected(idx) {
                continue;
            }
            sel_total += len;
            sel_done += st.file_progress.get(idx).copied().unwrap_or(0).min(len);
        }
        if !has_meta {
            sel_total = st.total_bytes;
            sel_done = st.progress_bytes;
        }
        let files = st
            .file_progress
            .iter()
            .enumerate()
            .map(|(idx, &done)| FileProgress { idx, done })
            .collect();
        let (down, up, connected, seen) = match &st.live {
            Some(l) => (
                l.download_speed.as_bytes(),
                l.upload_speed.as_bytes(),
                l.snapshot.peer_stats.live,
                l.snapshot.peer_stats.seen,
            ),
            None => (0, 0, 0, 0),
        };
        Some(TorrentStats {
            selected_total: sel_total,
            selected_done: sel_done,
            down_bps: down,
            up_bps: up,
            peers_connected: connected,
            peers_seen: seen,
            files,
            finished: st.finished,
            checking: matches!(st.state, TorrentStatsState::Initializing { .. }),
            has_meta,
            error: st.error.clone(),
        })
    }

    fn pieces(&self, hash: &InfoHash) -> Option<PieceMap> {
        let session = self.current()?;
        let (bf, len) = Api::new(session, None)
            .api_dump_haves(TorrentIdOrHash::Hash(id20(hash)))
            .ok()?;
        Some(PieceMap {
            piece_count: len,
            have: bf.as_raw_slice().to_vec(),
        })
    }

    async fn apply_settings(&self, new: TorrentSettings) -> Result<(), TorrentError> {
        let old = std::mem::replace(
            &mut *self.settings.lock().unwrap_or_else(|e| e.into_inner()),
            new.clone(),
        );
        let Some(session) = self.current() else {
            return Ok(());
        };
        session.ratelimits.set_download_bps(nz(new.download_bps));
        session.ratelimits.set_upload_bps(nz(new.upload_bps));
        if session_key(&old) != session_key(&new) {
            // A busy session keeps running; it restarts with the new port and proxy once idle.
            self.restart_pending.store(true, Ordering::SeqCst);
            self.restart_if_idle().await;
        }
        Ok(())
    }
}
