//! Shared fixture for the IPC tests: a manager on an in-memory store plus counters for the callbacks.
use odm_appcore::ipc::IpcContext;
use odm_appcore::offers::Offers;
use odm_appcore::summary::SpeedBoard;
use odm_appcore::Settings;
use odm_core::ipc::Request;
use odm_engine::{EngineConfig, ManagerHandle};
use odm_store::Store;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

pub struct Fixture {
    pub ctx: IpcContext,
    pub shows: Arc<AtomicUsize>,
    pub offers_seen: Arc<AtomicUsize>,
    pub _dir: TempDir,
}

impl Fixture {
    pub fn shows(&self) -> usize {
        self.shows.load(Ordering::SeqCst)
    }

    pub fn offers_seen(&self) -> usize {
        self.offers_seen.load(Ordering::SeqCst)
    }
}

pub async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let cfg = EngineConfig {
        download_dir: dir.path().to_path_buf(),
        ..Default::default()
    };
    let manager = ManagerHandle::start(cfg, Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let shows = Arc::new(AtomicUsize::new(0));
    let offers_seen = Arc::new(AtomicUsize::new(0));
    let (s, o) = (shows.clone(), offers_seen.clone());
    let settings = Settings::default();
    let ctx = IpcContext {
        manager,
        settings: Arc::new(Mutex::new(settings)),
        speeds: Arc::new(Mutex::new(SpeedBoard::default())),
        offers: Arc::new(Offers::default()),
        show_window: Box::new(move || {
            s.fetch_add(1, Ordering::SeqCst);
        }),
        on_offer: Box::new(move |_| {
            o.fetch_add(1, Ordering::SeqCst);
        }),
    };
    Fixture {
        ctx,
        shows,
        offers_seen,
        _dir: dir,
    }
}

pub fn req(cmd: &str, payload: Value) -> Request {
    Request {
        id: Some("r1".into()),
        cmd: cmd.into(),
        payload,
    }
}
