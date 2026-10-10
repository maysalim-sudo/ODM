//! The power runner against a real manager, with fake side effects: nothing here can
//! put this PC to sleep or shut it down.
use odm_appcore::dto::UiEvent;
use odm_appcore::power::{self, Power, PowerActions};
use odm_appcore::settings::AfterQueue;
use odm_appcore::Settings;
use odm_engine::{AddRequest, EngineConfig, ManagerHandle};
use odm_store::Store;
use odm_testserver::{Faults, TestServer};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Fake(Mutex<Vec<&'static str>>);

impl Fake {
    fn calls(&self) -> Vec<&'static str> {
        self.0.lock().unwrap().clone()
    }
}

impl PowerActions for Fake {
    fn keep_awake(&self, on: bool) {
        self.0
            .lock()
            .unwrap()
            .push(if on { "awake" } else { "may_sleep" });
    }
    fn sleep(&self) {
        self.0.lock().unwrap().push("sleep");
    }
    fn shutdown(&self) {
        self.0.lock().unwrap().push("shutdown");
    }
    fn cancel_shutdown(&self) {
        self.0.lock().unwrap().push("cancel_shutdown");
    }
    fn quit(&self) {
        self.0.lock().unwrap().push("quit");
    }
}

async fn wait_until(what: &str, ok: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ok() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn keeps_awake_while_downloading_then_runs_the_after_queue_action() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        1 << 20,
        Faults {
            bytes_per_sec: Some(2 << 20),
            ..Default::default()
        },
    )
    .await;
    let m = ManagerHandle::start(
        EngineConfig {
            download_dir: d.path().to_path_buf(),
            ..Default::default()
        },
        Store::open_in_memory().unwrap(),
    )
    .await
    .unwrap();
    let settings = Arc::new(Mutex::new(Settings {
        after_queue: AfterQueue::Shutdown,
        ..Settings::default()
    }));
    let p = Arc::new(Power::new(Fake::default()));
    let shown: Arc<Mutex<Vec<UiEvent>>> = Arc::default();
    let sink = shown.clone();
    tokio::spawn(power::run(
        m.clone(),
        settings.clone(),
        p.clone(),
        move |e| sink.lock().unwrap().push(e),
    ));
    tokio::time::sleep(Duration::from_millis(100)).await;

    m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_until("keep awake", || p.actions().calls().contains(&"awake")).await;
    wait_until("the shutdown", || p.actions().calls().contains(&"shutdown")).await;
    let calls = p.actions().calls();
    assert_eq!(calls, ["awake", "may_sleep", "shutdown"], "{calls:?}");
    assert_eq!(
        shown.lock().unwrap().clone(),
        vec![UiEvent::ShutdownPending { seconds: 60 }]
    );
    assert_eq!(p.cancel_shutdown(), Some(UiEvent::ShutdownCancelled));
    assert_eq!(p.actions().calls().last(), Some(&"cancel_shutdown"));
}

#[tokio::test(flavor = "multi_thread")]
async fn prevent_sleep_off_never_keeps_awake() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(256 << 10, Faults::default()).await;
    let m = ManagerHandle::start(
        EngineConfig {
            download_dir: d.path().to_path_buf(),
            ..Default::default()
        },
        Store::open_in_memory().unwrap(),
    )
    .await
    .unwrap();
    let settings = Arc::new(Mutex::new(Settings {
        prevent_sleep: false,
        after_queue: AfterQueue::Sleep,
        ..Settings::default()
    }));
    let p = Arc::new(Power::new(Fake::default()));
    tokio::spawn(power::run(m.clone(), settings, p.clone(), |_| {}));
    tokio::time::sleep(Duration::from_millis(100)).await;
    m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_until("sleep", || p.actions().calls().contains(&"sleep")).await;
    assert_eq!(p.actions().calls(), ["sleep"]);
}
