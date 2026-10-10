//! Download behaviour options: the file-exists policy, schedules, history and the
//! per-download speed limit.
use odm_core::DownloadState;
use odm_engine::schedule::{Clock, LocalNow, Window};
use odm_engine::{
    AddRequest, EngineConfig, EngineEvent, ExistsChoice, ExistsPolicy, ManagerHandle,
};
use odm_store::{DownloadRecord, Store};
use odm_testserver::{body, Faults, TestServer};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

const KB: u64 = 1 << 10;
const MB: u64 = 1 << 20;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

struct FakeClock(Mutex<LocalNow>);

impl FakeClock {
    fn new(now: LocalNow) -> Arc<Self> {
        Arc::new(Self(Mutex::new(now)))
    }
    fn set(&self, f: impl FnOnce(&mut LocalNow)) {
        f(&mut self.0.lock().unwrap());
    }
}

impl Clock for FakeClock {
    fn now(&self) -> LocalNow {
        *self.0.lock().unwrap()
    }
}

fn real_ms() -> i64 {
    odm_store::now_ms()
}

/// Monday at `h`:`m`, with the real wall time (history is not under test here).
fn monday(h: u16, m: u16) -> LocalNow {
    LocalNow {
        unix_ms: real_ms(),
        weekday: 0,
        minute: h * 60 + m,
    }
}

fn cfg(dir: &Path) -> EngineConfig {
    EngineConfig {
        download_dir: dir.to_path_buf(),
        progress_interval: Duration::from_millis(50),
        persist_interval: Duration::from_millis(200),
        timer_interval: Duration::from_millis(50),
        ..EngineConfig::default()
    }
}

fn named(url: String, dir: &Path, name: &str) -> AddRequest {
    let mut r = AddRequest::new(url);
    r.filename = Some(name.into());
    r.save_dir = Some(dir.to_path_buf());
    r
}

async fn wait_for<F: Fn(&DownloadRecord) -> bool>(
    store: &Store,
    id: &str,
    what: &str,
    secs: u64,
    ok: F,
) -> DownloadRecord {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let r = store.get_download(id).await.unwrap().expect("record");
        if ok(&r) {
            return r;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; state {:?}, reason {:?}, error {:?}",
            r.state,
            r.pause_reason,
            r.error_msg
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_state(store: &Store, id: &str, want: DownloadState) -> DownloadRecord {
    wait_for(store, id, &format!("{want:?}"), 20, |r| r.state == want).await
}

async fn next_prompt(rx: &mut broadcast::Receiver<EngineEvent>) -> (String, String) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, rx.recv()).await {
            Ok(Ok(EngineEvent::ExistsPrompt { id, name })) => return (id, name),
            Ok(Ok(_)) => {}
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => {}
            other => panic!("no exists prompt: {other:?}"),
        }
    }
}

async fn start(c: EngineConfig) -> ManagerHandle {
    ManagerHandle::start(c, Store::open_in_memory().unwrap())
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn exists_rename() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("data.bin"), b"old").unwrap();
    let s = TestServer::start(MB, Faults::default()).await;
    let m = start(EngineConfig {
        on_exists: ExistsPolicy::Rename,
        ..cfg(d.path())
    })
    .await;
    let id = m
        .add(named(s.url("/file"), d.path(), "data.bin"))
        .await
        .unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed).await;
    assert_eq!(r.filename, "data (1).bin");
    assert_eq!(std::fs::read(d.path().join("data.bin")).unwrap(), b"old");
    assert_eq!(
        std::fs::read(d.path().join("data (1).bin")).unwrap(),
        body(MB)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn exists_overwrite() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("data.bin"), b"old").unwrap();
    let s = TestServer::start(MB, Faults::default()).await;
    let m = start(EngineConfig {
        on_exists: ExistsPolicy::Overwrite,
        ..cfg(d.path())
    })
    .await;
    let id = m
        .add(named(s.url("/file"), d.path(), "data.bin"))
        .await
        .unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed).await;
    assert_eq!(r.filename, "data.bin");
    assert_eq!(std::fs::read(d.path().join("data.bin")).unwrap(), body(MB));
    assert!(!d.path().join("data (1).bin").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn exists_ask_waits_for_answer() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("data.bin"), b"old").unwrap();
    let s = TestServer::start(MB, Faults::default()).await;
    // One download at a time: the waiting one must not hold the only slot.
    let m = start(EngineConfig {
        on_exists: ExistsPolicy::Ask,
        max_concurrent: 1,
        ..cfg(d.path())
    })
    .await;
    let mut rx = m.subscribe();
    let id = m
        .add(named(s.url("/file"), d.path(), "data.bin"))
        .await
        .unwrap();
    let (prompt_id, name) = next_prompt(&mut rx).await;
    assert_eq!(
        (prompt_id.as_str(), name.as_str()),
        (id.as_str(), "data.bin")
    );

    let r = wait_state(m.store(), &id, DownloadState::Paused).await;
    assert_eq!(r.pause_reason.as_deref(), Some("exists"));
    assert_eq!(std::fs::read(d.path().join("data.bin")).unwrap(), b"old");
    assert_eq!(m.active_count().await.unwrap(), 0);

    // It waits instead of failing, and the queue goes on meanwhile.
    let other = m
        .add(named(s.url("/file"), d.path(), "other.bin"))
        .await
        .unwrap();
    wait_state(m.store(), &other, DownloadState::Completed).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let r = m.store().get_download(&id).await.unwrap().unwrap();
    assert_eq!(
        (r.state, r.pause_reason.as_deref()),
        (DownloadState::Paused, Some("exists"))
    );

    // Resume all leaves it alone; Resume asks again instead of downloading again.
    m.resume_all().await.unwrap();
    m.resume(&id).await.unwrap();
    let (again, _) = next_prompt(&mut rx).await;
    assert_eq!(again, id);
    assert_eq!(
        m.store().get_download(&id).await.unwrap().unwrap().state,
        DownloadState::Paused
    );

    m.answer_exists(&id, ExistsChoice::KeepBoth).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed).await;
    assert_eq!(r.filename, "data (1).bin");
    assert_eq!(r.pause_reason, None);
    assert_eq!(std::fs::read(d.path().join("data.bin")).unwrap(), b"old");
    assert_eq!(
        std::fs::read(d.path().join("data (1).bin")).unwrap(),
        body(MB)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn exists_ask_replace_and_cancel() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.bin"), b"old a").unwrap();
    std::fs::write(d.path().join("b.bin"), b"old b").unwrap();
    let s = TestServer::start(MB, Faults::default()).await;
    let m = start(EngineConfig {
        on_exists: ExistsPolicy::Ask,
        ..cfg(d.path())
    })
    .await;
    let mut rx = m.subscribe();
    let a = m
        .add(named(s.url("/file"), d.path(), "a.bin"))
        .await
        .unwrap();
    let b = m
        .add(named(s.url("/file"), d.path(), "b.bin"))
        .await
        .unwrap();
    let mut seen = vec![next_prompt(&mut rx).await.0, next_prompt(&mut rx).await.0];
    seen.sort();
    let mut want = vec![a.clone(), b.clone()];
    want.sort();
    assert_eq!(seen, want);

    m.answer_exists(&a, ExistsChoice::Replace).await.unwrap();
    let ra = wait_state(m.store(), &a, DownloadState::Completed).await;
    assert_eq!(ra.filename, "a.bin");
    assert_eq!(std::fs::read(d.path().join("a.bin")).unwrap(), body(MB));

    m.answer_exists(&b, ExistsChoice::Cancel).await.unwrap();
    wait_state(m.store(), &b, DownloadState::Cancelled).await;
    assert_eq!(std::fs::read(d.path().join("b.bin")).unwrap(), b"old b");
    let left: Vec<String> = std::fs::read_dir(d.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".odm-part"))
        .collect();
    assert!(left.is_empty(), "part files left: {left:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn history_removes_old_completed_records_with_a_fake_clock() {
    let d = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let now = real_ms();
    let dir = d.path().to_string_lossy().into_owned();
    for (id, state, age_ms) in [
        ("old", DownloadState::Completed, 10 * DAY_MS),
        // Six and a half days: old enough half a day from now.
        ("recent", DownloadState::Completed, 6 * DAY_MS + DAY_MS / 2),
        ("old_failed", DownloadState::Failed, 10 * DAY_MS),
    ] {
        let mut r = DownloadRecord::new_file(id, "https://e.com/x", &format!("{id}.bin"), &dir);
        r.state = state;
        r.finished_at = Some(now - age_ms);
        store.insert_download(r).await.unwrap();
    }
    std::fs::write(d.path().join("old.bin"), b"keep me").unwrap();

    let clock = FakeClock::new(LocalNow {
        unix_ms: now,
        weekday: 2,
        minute: 600,
    });
    let m = ManagerHandle::start_with_clock(
        EngineConfig {
            history_days: 7,
            ..cfg(d.path())
        },
        store,
        clock.clone(),
    )
    .await
    .unwrap();
    let mut rx = m.subscribe();
    let deadline = Instant::now() + Duration::from_secs(10);
    while m.store().get_download("old").await.unwrap().is_some() {
        assert!(Instant::now() < deadline, "the old record was not removed");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Records only: the file stays.
    assert_eq!(std::fs::read(d.path().join("old.bin")).unwrap(), b"keep me");
    assert!(m.store().get_download("recent").await.unwrap().is_some());
    assert!(m
        .store()
        .get_download("old_failed")
        .await
        .unwrap()
        .is_some());

    while rx.try_recv().is_ok() {}

    // The pass runs once a day. 23 hours later "recent" is old enough, but only the
    // next daily pass removes it.
    clock.set(|n| n.unix_ms = now + 23 * DAY_MS / 24);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(m.store().get_download("recent").await.unwrap().is_some());
    clock.set(|n| n.unix_ms = now + 25 * DAY_MS / 24);
    let deadline = Instant::now() + Duration::from_secs(10);
    while m.store().get_download("recent").await.unwrap().is_some() {
        assert!(
            Instant::now() < deadline,
            "the second daily pass did not run"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut removed = Vec::new();
    while let Ok(e) = rx.try_recv() {
        if let EngineEvent::Removed { id } = e {
            removed.push(id);
        }
    }
    assert_eq!(removed, vec!["recent".to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn queue_schedule_gates_starts_and_pauses_at_the_stop_time() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        4 * MB,
        Faults {
            bytes_per_sec: Some(512 * KB),
            ..Default::default()
        },
    )
    .await;
    let clock = FakeClock::new(monday(12, 0));
    let m = ManagerHandle::start_with_clock(
        EngineConfig {
            queue_window: Some(Window {
                days: [true, false, false, false, false, false, false],
                from: 13 * 60,
                to: 14 * 60,
            }),
            ..cfg(d.path())
        },
        Store::open_in_memory().unwrap(),
        clock.clone(),
    )
    .await
    .unwrap();
    let id = m
        .add(named(s.url("/file"), d.path(), "q.bin"))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        m.store().get_download(&id).await.unwrap().unwrap().state,
        DownloadState::Queued,
        "nothing starts outside the window"
    );

    clock.set(|n| n.minute = 13 * 60);
    wait_for(m.store(), &id, "a start", 10, |r| r.state.is_active()).await;

    clock.set(|n| n.minute = 14 * 60);
    let r = wait_state(m.store(), &id, DownloadState::Paused).await;
    assert_eq!(r.pause_reason.as_deref(), Some("schedule"));

    // Resume all does not fight the schedule.
    m.resume_all().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let r = m.store().get_download(&id).await.unwrap().unwrap();
    assert_eq!(
        (r.state, r.pause_reason.as_deref()),
        (DownloadState::Paused, Some("schedule"))
    );

    // Next start time: it carries on by itself.
    clock.set(|n| n.minute = 13 * 60 + 1);
    wait_for(m.store(), &id, "the restart", 10, |r| r.state.is_active()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn queue_done_is_sent_once_the_queue_is_empty() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(256 * KB, Faults::default()).await;
    let m = start(cfg(d.path())).await;
    let mut rx = m.subscribe();
    let a = m
        .add(named(s.url("/file"), d.path(), "a.bin"))
        .await
        .unwrap();
    let b = m
        .add(named(s.url("/file"), d.path(), "b.bin"))
        .await
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, rx.recv()).await {
            Ok(Ok(EngineEvent::QueueDone)) => break,
            Ok(Ok(_)) => {}
            other => panic!("no queue-done event: {other:?}"),
        }
    }
    for id in [&a, &b] {
        assert_eq!(
            m.store().get_download(id).await.unwrap().unwrap().state,
            DownloadState::Completed
        );
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    while let Ok(e) = rx.try_recv() {
        assert_ne!(e, EngineEvent::QueueDone, "sent twice");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn per_download_speed_limit() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(512 * KB, Faults::default()).await;
    let m = start(cfg(d.path())).await;
    let mut req = named(s.url("/file"), d.path(), "slow.bin");
    req.speed_limit = Some(200 * KB);
    let t = Instant::now();
    let id = m.add(req).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed).await;
    assert_eq!(r.speed_limit, Some(200 * KB));
    assert!(
        t.elapsed() >= Duration::from_millis(1400),
        "finished in {:?}; the limit was ignored",
        t.elapsed()
    );

    m.set_download_limit(&id, None).await.unwrap();
    assert_eq!(
        m.store()
            .get_download(&id)
            .await
            .unwrap()
            .unwrap()
            .speed_limit,
        None
    );
}

/// No QueueDone arrives within `ms`.
async fn no_queue_done(rx: &mut broadcast::Receiver<EngineEvent>, ms: u64) {
    let until = Instant::now() + Duration::from_millis(ms);
    loop {
        let left = until.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, rx.recv()).await {
            Err(_) => return,
            Ok(Ok(EngineEvent::QueueDone)) => panic!("QueueDone with work still left"),
            Ok(_) => {}
        }
    }
}

async fn wait_queue_done(rx: &mut broadcast::Receiver<EngineEvent>) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, rx.recv()).await {
            Ok(Ok(EngineEvent::QueueDone)) => return,
            Ok(_) => {}
            Err(_) => panic!("no queue-done event"),
        }
    }
}

async fn slow_server() -> TestServer {
    TestServer::start(
        8 * MB,
        Faults {
            bytes_per_sec: Some(256 * KB),
            ..Default::default()
        },
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn pausing_the_last_download_is_not_queue_done() {
    let d = tempfile::tempdir().unwrap();
    let fast = TestServer::start(64 * KB, Faults::default()).await;
    let slow = slow_server().await;
    let m = start(cfg(d.path())).await;
    let mut rx = m.subscribe();
    let b = m
        .add(named(slow.url("/file"), d.path(), "b.bin"))
        .await
        .unwrap();
    let a = m
        .add(named(fast.url("/file"), d.path(), "a.bin"))
        .await
        .unwrap();
    wait_state(m.store(), &a, DownloadState::Completed).await;
    m.pause(&b).await.unwrap();
    wait_state(m.store(), &b, DownloadState::Paused).await;
    no_queue_done(&mut rx, 400).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_schedule_stop_time_is_not_queue_done() {
    let d = tempfile::tempdir().unwrap();
    let fast = TestServer::start(64 * KB, Faults::default()).await;
    let slow = slow_server().await;
    let clock = FakeClock::new(monday(13, 0));
    let m = ManagerHandle::start_with_clock(
        EngineConfig {
            queue_window: Some(Window {
                days: [true; 7],
                from: 13 * 60,
                to: 14 * 60,
            }),
            ..cfg(d.path())
        },
        Store::open_in_memory().unwrap(),
        clock.clone(),
    )
    .await
    .unwrap();
    let mut rx = m.subscribe();
    let b = m
        .add(named(slow.url("/file"), d.path(), "b.bin"))
        .await
        .unwrap();
    let a = m
        .add(named(fast.url("/file"), d.path(), "a.bin"))
        .await
        .unwrap();
    wait_state(m.store(), &a, DownloadState::Completed).await;
    clock.set(|n| n.minute = 14 * 60);
    let r = wait_state(m.store(), &b, DownloadState::Paused).await;
    assert_eq!(r.pause_reason.as_deref(), Some("schedule"));
    no_queue_done(&mut rx, 400).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_the_waiting_download_ends_the_queue() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("b.bin"), b"old").unwrap();
    let s = TestServer::start(64 * KB, Faults::default()).await;
    let m = start(EngineConfig {
        on_exists: ExistsPolicy::Ask,
        max_concurrent: 1,
        ..cfg(d.path())
    })
    .await;
    let mut rx = m.subscribe();
    let b = m
        .add(named(s.url("/file"), d.path(), "b.bin"))
        .await
        .unwrap();
    next_prompt(&mut rx).await;
    let a = m
        .add(named(s.url("/file"), d.path(), "a.bin"))
        .await
        .unwrap();
    wait_state(m.store(), &a, DownloadState::Completed).await;
    no_queue_done(&mut rx, 200).await;
    // Cancelled from the list, not through the prompt.
    m.cancel(&b, false).await.unwrap();
    wait_queue_done(&mut rx).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_the_waiting_download_ends_the_queue() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("b.bin"), b"old").unwrap();
    let s = TestServer::start(64 * KB, Faults::default()).await;
    let m = start(EngineConfig {
        on_exists: ExistsPolicy::Ask,
        max_concurrent: 1,
        ..cfg(d.path())
    })
    .await;
    let mut rx = m.subscribe();
    let b = m
        .add(named(s.url("/file"), d.path(), "b.bin"))
        .await
        .unwrap();
    next_prompt(&mut rx).await;
    let a = m
        .add(named(s.url("/file"), d.path(), "a.bin"))
        .await
        .unwrap();
    wait_state(m.store(), &a, DownloadState::Completed).await;
    m.remove(&b, false).await.unwrap();
    wait_queue_done(&mut rx).await;
}
