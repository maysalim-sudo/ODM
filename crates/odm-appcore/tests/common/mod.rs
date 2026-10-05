//! Shared fixture for the IPC tests: a manager on an in-memory store plus counters for the callbacks.
use odm_appcore::confirm::Pending;
use odm_appcore::integration::Contact;
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
    pub ctx: Arc<IpcContext>,
    pub shows: Arc<AtomicUsize>,
    pub offers_seen: Arc<AtomicUsize>,
    confirms: Arc<Mutex<Vec<String>>>,
    pub _dir: TempDir,
}

impl Fixture {
    pub fn shows(&self) -> usize {
        self.shows.load(Ordering::SeqCst)
    }

    pub fn offers_seen(&self) -> usize {
        self.offers_seen.load(Ordering::SeqCst)
    }

    /// Ids passed to `on_confirm`, in order.
    pub fn confirms(&self) -> Vec<String> {
        self.confirms.lock().unwrap().clone()
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
    let confirms = Arc::new(Mutex::new(Vec::new()));
    let c = confirms.clone();
    // The confirm dialog is on by default; most tests want the plain add-and-start flow.
    let settings = Settings {
        confirm_browser_downloads: false,
        ..Settings::default()
    };
    let ctx = Arc::new(IpcContext {
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
        pending: Arc::new(Pending::default()),
        on_confirm: Box::new(move |id| {
            c.lock().unwrap().push(id.to_string());
        }),
        contact: Arc::new(Contact::default()),
    });
    Fixture {
        ctx,
        shows,
        offers_seen,
        confirms,
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
