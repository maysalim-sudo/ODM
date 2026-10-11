//! The power runner against a real manager, with fake side effects: nothing here can
//! put this PC to sleep or shut it down.
use odm_appcore::dto::UiEvent;
use odm_appcore::power::{self, Power, PowerActions};
use odm_appcore::settings::AfterQueue;
use odm_appcore::Settings;
use odm_engine::config::ProxyConfig;
use odm_engine::{AddRequest, EngineConfig, ManagerHandle};
use odm_store::Store;
use odm_testserver::{Faults, TestServer};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Fake(Mutex<Vec<&'static str>>, Duration);

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
        // The real SetSuspendState returns only after the PC wakes up.
        std::thread::sleep(self.1);
    }
    fn shutdown(&self) -> Result<(), String> {
        self.0.lock().unwrap().push("shutdown");
        Ok(())
    }
    fn cancel_shutdown(&self) {
        self.0.lock().unwrap().push("cancel_shutdown");
    }
    fn quit(&self) {
        self.0.lock().unwrap().push("quit");
    }
    fn announce(&self, _action: AfterQueue) {
        self.0.lock().unwrap().push("announce");
    }
}

type Shown = Arc<Mutex<Vec<UiEvent>>>;

/// Starts the runner with a short countdown; returns what it showed.
fn follow(
    m: &ManagerHandle,
    settings: Arc<Mutex<Settings>>,
    p: &Arc<Power<Fake>>,
    delay: Duration,
) -> Shown {
    let shown: Shown = Arc::default();
    let sink = shown.clone();
    tokio::spawn(power::run_with_delay(
        m.clone(),
        settings,
        p.clone(),
        move |e| sink.lock().unwrap().push(e),
        delay,
    ));
    shown
}

async fn manager(dir: &std::path::Path) -> ManagerHandle {
    ManagerHandle::start(
        EngineConfig {
            download_dir: dir.to_path_buf(),
            proxy: ProxyConfig::Direct,
            ..Default::default()
        },
        Store::open_in_memory().unwrap(),
    )
    .await
    .unwrap()
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
            proxy: ProxyConfig::Direct,
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
        vec![UiEvent::AfterQueuePending {
            action: AfterQueue::Shutdown,
            seconds: 60
        }]
    );
    assert_eq!(p.cancel_pending(), Some(UiEvent::AfterQueueCancelled));
    assert_eq!(p.actions().calls().last(), Some(&"cancel_shutdown"));
}

#[tokio::test(flavor = "multi_thread")]
async fn prevent_sleep_off_never_keeps_awake() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(256 << 10, Faults::default()).await;
    let m = ManagerHandle::start(
        EngineConfig {
            download_dir: d.path().to_path_buf(),
            proxy: ProxyConfig::Direct,
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
    follow(&m, settings, &p, Duration::from_millis(300));
    tokio::time::sleep(Duration::from_millis(100)).await;
    m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_until("sleep", || p.actions().calls().contains(&"sleep")).await;
    assert_eq!(p.actions().calls(), ["announce", "sleep"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sleep_that_blocks_does_not_stall_the_runner() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(256 << 10, Faults::default()).await;
    let slow = TestServer::start(
        4 << 20,
        Faults {
            bytes_per_sec: Some(512 << 10),
            ..Default::default()
        },
    )
    .await;
    let m = ManagerHandle::start(
        EngineConfig {
            download_dir: d.path().to_path_buf(),
            proxy: ProxyConfig::Direct,
            ..Default::default()
        },
        Store::open_in_memory().unwrap(),
    )
    .await
    .unwrap();
    let settings = Arc::new(Mutex::new(Settings {
        after_queue: AfterQueue::Sleep,
        ..Settings::default()
    }));
    let p = Arc::new(Power::new(Fake(Mutex::default(), Duration::from_secs(4))));
    follow(&m, settings, &p, Duration::from_millis(200));
    tokio::time::sleep(Duration::from_millis(100)).await;
    m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_until("sleep", || p.actions().calls().contains(&"sleep")).await;
    // While "asleep", a new download still keeps the PC awake at once.
    let t = Instant::now();
    m.add(AddRequest::new(slow.url("/file"))).await.unwrap();
    wait_until("awake again", || {
        p.actions()
            .calls()
            .iter()
            .filter(|c| **c == "awake")
            .count()
            == 2
    })
    .await;
    assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
}

/// Sleep and quit count down like a shutdown; a download that starts meanwhile cancels them.
#[tokio::test(flavor = "multi_thread")]
async fn sleep_and_quit_wait_and_a_new_download_cancels_them() {
    for (after, call) in [(AfterQueue::Sleep, "sleep"), (AfterQueue::Quit, "quit")] {
        let d = tempfile::tempdir().unwrap();
        let s = TestServer::start(64 << 10, Faults::default()).await;
        let slow = TestServer::start(
            4 << 20,
            Faults {
                bytes_per_sec: Some(256 << 10),
                ..Default::default()
            },
        )
        .await;
        let m = manager(d.path()).await;
        let settings = Arc::new(Mutex::new(Settings {
            prevent_sleep: false,
            after_queue: after,
            ..Settings::default()
        }));
        let p = Arc::new(Power::new(Fake::default()));
        let delay = Duration::from_millis(1500);
        let shown = follow(&m, settings, &p, delay);
        tokio::time::sleep(Duration::from_millis(100)).await;

        // First round: the countdown runs out, then the action runs.
        m.add(AddRequest::new(s.url("/file"))).await.unwrap();
        wait_until("the countdown", || !shown.lock().unwrap().is_empty()).await;
        let started = Instant::now();
        assert_eq!(
            shown.lock().unwrap()[0],
            UiEvent::AfterQueuePending {
                action: after,
                seconds: 60
            }
        );
        assert_eq!(p.actions().calls(), ["announce"], "{after:?} waits");
        wait_until("the action", || p.actions().calls().contains(&call)).await;
        assert!(
            started.elapsed() >= Duration::from_millis(1200),
            "{after:?} came early"
        );
        wait_until("done", || {
            shown.lock().unwrap().last() == Some(&UiEvent::AfterQueueDone)
        })
        .await;

        // Second round: a new download during the countdown cancels it.
        shown.lock().unwrap().clear();
        m.add(AddRequest::new(s.url("/file"))).await.unwrap();
        wait_until("the second countdown", || {
            shown
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, UiEvent::AfterQueuePending { .. }))
        })
        .await;
        m.add(AddRequest::new(slow.url("/file"))).await.unwrap();
        wait_until("the cancel", || {
            shown
                .lock()
                .unwrap()
                .contains(&UiEvent::AfterQueueCancelled)
        })
        .await;
        tokio::time::sleep(delay + Duration::from_millis(500)).await;
        let calls = p.actions().calls();
        assert_eq!(
            calls.iter().filter(|c| **c == call).count(),
            1,
            "{after:?} ran after the cancel: {calls:?}"
        );
        assert_eq!(p.pending(), None);
    }
}

/// Cancel in the dialog stops the timer; nothing runs when it would have run out.
#[tokio::test(flavor = "multi_thread")]
async fn cancelling_the_countdown_prevents_the_action() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(64 << 10, Faults::default()).await;
    let m = manager(d.path()).await;
    let settings = Arc::new(Mutex::new(Settings {
        prevent_sleep: false,
        after_queue: AfterQueue::Quit,
        ..Settings::default()
    }));
    let p = Arc::new(Power::new(Fake::default()));
    let delay = Duration::from_millis(600);
    let shown = follow(&m, settings, &p, delay);
    tokio::time::sleep(Duration::from_millis(100)).await;
    m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_until("the countdown", || !shown.lock().unwrap().is_empty()).await;
    assert_eq!(p.cancel_pending(), Some(UiEvent::AfterQueueCancelled));
    tokio::time::sleep(delay * 2).await;
    assert_eq!(p.actions().calls(), ["announce"]);
}
