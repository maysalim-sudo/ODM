use odm_appcore::bridge;
use odm_appcore::dto::{to_dto, DownloadDto, UiEvent};
use odm_core::DownloadState;
use odm_engine::{AddRequest, EngineConfig, EngineEvent, ManagerHandle, ProgressSnapshot};
use odm_store::{DownloadRecord, Store};
use odm_testserver::{Faults, TestServer};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

type Log = Arc<Mutex<Vec<UiEvent>>>;

fn sink() -> (Log, impl Fn(UiEvent) + Send + 'static) {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let l2 = log.clone();
    (log, move |e| l2.lock().unwrap().push(e))
}

fn dto(id: &str, state: DownloadState) -> DownloadDto {
    let mut r = DownloadRecord::new_file(id, "https://e.com/f", "f", "C:/dl");
    r.state = state;
    to_dto(&r, 0)
}

fn progress(id: &str, bytes: u64) -> EngineEvent {
    EngineEvent::Progress(ProgressSnapshot {
        id: id.into(),
        bytes,
        total: Some(100),
        speed_bps: 1,
        eta_s: None,
        chunks: vec![],
        segments: None,
    })
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(150)).await;
}

#[tokio::test]
async fn progress_is_coalesced_into_batches() {
    let (tx, rx) = broadcast::channel(64);
    let (log, emit) = sink();
    let task = tokio::spawn(bridge::pump(
        rx,
        |_id| async { None },
        Duration::from_millis(50),
        emit,
    ));
    for b in 1..=10 {
        tx.send(progress("a", b)).unwrap();
        tx.send(progress("b", b * 2)).unwrap();
    }
    settle().await;
    drop(tx);
    task.await.unwrap();

    let events = log.lock().unwrap().clone();
    assert!(!events.is_empty());
    // Far fewer batches than the 20 progress events, and only the latest snapshot per id survives.
    assert!(events.len() <= 4, "{}", events.len());
    let UiEvent::ProgressBatch { items } = events.last().unwrap() else {
        panic!("{events:?}")
    };
    let ids: Vec<&str> = items.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "b"]);
    assert_eq!((items[0].bytes, items[1].bytes), (10, 20));
}

#[tokio::test]
async fn state_events_become_upserts_and_drop_stale_progress() {
    let (tx, rx) = broadcast::channel(64);
    let (log, emit) = sink();
    // Built once: `created_at` is the wall clock, so two calls would differ.
    let expected = dto("a", DownloadState::Completed);
    let looked_up = expected.clone();
    let lookup = move |_id: String| {
        let d = looked_up.clone();
        async move { Some(d) }
    };
    let task = tokio::spawn(bridge::pump(rx, lookup, Duration::from_secs(60), emit));
    tx.send(progress("a", 5)).unwrap();
    tx.send(EngineEvent::State {
        id: "a".into(),
        state: DownloadState::Completed,
        error_code: None,
        error_message: None,
    })
    .unwrap();
    tx.send(EngineEvent::Removed { id: "b".into() }).unwrap();
    settle().await;
    drop(tx);
    task.await.unwrap();

    let events = log.lock().unwrap().clone();
    assert_eq!(
        events,
        vec![
            UiEvent::Upsert {
                download: Box::new(expected)
            },
            UiEvent::Removed { id: "b".into() },
        ]
    );
}

#[tokio::test]
async fn unknown_ids_emit_nothing() {
    let (tx, rx) = broadcast::channel(8);
    let (log, emit) = sink();
    let task = tokio::spawn(bridge::pump(
        rx,
        |_id| async { None },
        Duration::from_secs(60),
        emit,
    ));
    tx.send(EngineEvent::Added { id: "ghost".into() }).unwrap();
    settle().await;
    drop(tx);
    task.await.unwrap();
    assert!(log.lock().unwrap().is_empty());
}

#[tokio::test]
async fn lagged_emits_resync() {
    let (tx, rx) = broadcast::channel(4);
    // Overflow the channel before the bridge starts reading.
    for b in 0..50 {
        tx.send(progress("a", b)).unwrap();
    }
    let (log, emit) = sink();
    let task = tokio::spawn(bridge::pump(
        rx,
        |_id| async { None },
        Duration::from_secs(60),
        emit,
    ));
    settle().await;
    drop(tx);
    task.await.unwrap();
    let events = log.lock().unwrap().clone();
    assert_eq!(events.first(), Some(&UiEvent::Resync), "{events:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_real_download_produces_upserts_and_batches() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        4 << 20,
        Faults {
            bytes_per_sec: Some(2 << 20),
            ..Default::default()
        },
    )
    .await;
    let cfg = EngineConfig {
        download_dir: d.path().to_path_buf(),
        progress_interval: Duration::from_millis(20),
        // Hermetic behind a system proxy: the test servers are on this PC.
        proxy: odm_engine::config::ProxyConfig::Direct,
        ..Default::default()
    };
    let m = ManagerHandle::start(cfg, Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let (log, emit) = sink();
    let task = tokio::spawn(bridge::run(m.clone(), Duration::from_millis(100), emit));

    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let done = log.lock().unwrap().iter().any(|e| {
            matches!(e, UiEvent::Upsert { download } if download.id == id && download.state == DownloadState::Completed)
        });
        if done {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "no completed upsert");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    task.abort();

    let events = log.lock().unwrap().clone();
    // The first thing the UI hears about a new download is its row. (Its state may
    // already be past `queued`: the engine starts it right after announcing it.)
    assert!(
        matches!(&events[0], UiEvent::Upsert { download } if download.id == id),
        "{:?}",
        events[0]
    );
    let batches = events
        .iter()
        .filter(|e| matches!(e, UiEvent::ProgressBatch { .. }))
        .count();
    assert!(batches >= 1, "no progress batches");
    // ~1 s of download at 20 ms progress ticks is ~50 engine events; batching at 100 ms keeps it near 10.
    assert!(
        batches <= 30,
        "{batches} batches: progress is not being coalesced"
    );
    let last = events.iter().rev().find_map(|e| match e {
        UiEvent::Upsert { download } => Some((**download).clone()),
        _ => None,
    });
    let last = last.unwrap();
    assert_eq!(
        (last.state, last.bytes),
        (DownloadState::Completed, 4 << 20)
    );
}

async fn upsert_bytes_after_progress(state: DownloadState) -> u64 {
    let (tx, rx) = broadcast::channel(64);
    let (log, emit) = sink();
    let mut stored = dto("a", state);
    stored.bytes = 38;
    let lookup = move |_id: String| {
        let d = stored.clone();
        async move { Some(d) }
    };
    let task = tokio::spawn(bridge::pump(rx, lookup, Duration::from_secs(60), emit));
    tx.send(progress("a", 41)).unwrap();
    tx.send(EngineEvent::State {
        id: "a".into(),
        state,
        error_code: None,
        error_message: None,
    })
    .unwrap();
    settle().await;
    drop(tx);
    task.await.unwrap();
    let events = log.lock().unwrap().clone();
    let [UiEvent::Upsert { download }] = events.as_slice() else {
        panic!("{events:?}")
    };
    download.bytes
}

#[tokio::test]
async fn upsert_never_reports_fewer_bytes_than_pending_progress() {
    assert_eq!(
        upsert_bytes_after_progress(DownloadState::Downloading).await,
        41
    );
    assert_eq!(upsert_bytes_after_progress(DownloadState::Paused).await, 38);
}

#[tokio::test]
async fn exists_prompts_reach_the_ui_and_queue_done_does_not() {
    let (tx, rx) = broadcast::channel(8);
    let (log, emit) = sink();
    let task = tokio::spawn(bridge::pump(
        rx,
        |_id| async { None },
        Duration::from_secs(60),
        emit,
    ));
    tx.send(EngineEvent::ExistsPrompt {
        id: "a".into(),
        name: "f.zip".into(),
    })
    .unwrap();
    tx.send(EngineEvent::QueueDone).unwrap();
    settle().await;
    drop(tx);
    task.await.unwrap();
    assert_eq!(
        log.lock().unwrap().clone(),
        vec![UiEvent::ExistsPrompt {
            id: "a".into(),
            name: "f.zip".into()
        }]
    );
}
