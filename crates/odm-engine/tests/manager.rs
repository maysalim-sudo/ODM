use odm_core::DownloadState;
use odm_engine::{AddRequest, EngineConfig, EngineEvent, ManagerHandle};
use odm_store::{DownloadRecord, Store};
use odm_testserver::{body, Faults, TestServer};
use std::path::Path;
use std::time::{Duration, Instant};

const MB: u64 = 1 << 20;

fn cfg(dir: &Path) -> EngineConfig {
    EngineConfig {
        download_dir: dir.to_path_buf(),
        progress_interval: Duration::from_millis(50),
        persist_interval: Duration::from_millis(200),
        ..EngineConfig::default()
    }
}

async fn wait_state(store: &Store, id: &str, want: DownloadState, secs: u64) -> DownloadRecord {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let r = store.get_download(id).await.unwrap().expect("record");
        if r.state == want {
            return r;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {want:?}, state is {:?} ({:?})",
            r.state,
            r.error_msg
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_bytes(store: &Store, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let w: u64 = store
            .get_chunks(id)
            .await
            .unwrap()
            .iter()
            .map(|c| c.written)
            .sum();
        if w > 0 {
            return;
        }
        assert!(Instant::now() < deadline, "no progress persisted");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn final_path(r: &DownloadRecord) -> std::path::PathBuf {
    Path::new(&r.save_dir).join(&r.filename)
}

#[tokio::test(flavor = "multi_thread")]
async fn completes_and_emits_events() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(2 * MB, Faults::default()).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let mut rx = m.subscribe();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed, 20).await;
    assert_eq!(std::fs::read(final_path(&r)).unwrap(), body(2 * MB));

    let mut seen = Vec::new();
    while let Ok(e) = rx.try_recv() {
        seen.push(e);
    }
    let states: Vec<DownloadState> = seen
        .iter()
        .filter_map(|e| match e {
            EngineEvent::State { id: i, state, .. } if *i == id => Some(*state),
            _ => None,
        })
        .collect();
    assert!(matches!(seen.first(), Some(EngineEvent::Added { .. })));
    assert_eq!(
        states,
        vec![
            DownloadState::Probing,
            DownloadState::Downloading,
            DownloadState::Completed
        ]
    );
    assert!(seen.iter().any(|e| matches!(e, EngineEvent::Progress(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn queue_respects_max_concurrent() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        MB,
        Faults {
            bytes_per_sec: Some(2 * MB),
            ..Default::default()
        },
    )
    .await;
    let mut c = cfg(d.path());
    c.max_concurrent = 2;
    let m = ManagerHandle::start(c, Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let mut ids = Vec::new();
    for _ in 0..4 {
        let mut r = AddRequest::new(s.url("/file"));
        r.max_connections = Some(1);
        ids.push(m.add(r).await.unwrap());
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut peak = 0;
    loop {
        let all = m.store().list_downloads().await.unwrap();
        let active = all.iter().filter(|r| r.state.is_active()).count();
        peak = peak.max(active);
        assert!(active <= 2, "{active} active");
        if all.iter().all(|r| r.state == DownloadState::Completed) {
            break;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(peak, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_then_immediate_resume_is_safe() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        12 * MB,
        Faults {
            bytes_per_sec: Some(MB / 2),
            ..Default::default()
        },
    )
    .await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let mut rx = m.subscribe();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_bytes(m.store(), &id).await;
    m.pause(&id).await.unwrap();
    m.resume(&id).await.unwrap(); // arrives while the task is still stopping
    let r = wait_state(m.store(), &id, DownloadState::Completed, 60).await;
    assert_eq!(std::fs::read(final_path(&r)).unwrap(), body(12 * MB));

    let mut states = Vec::new();
    while let Ok(e) = rx.try_recv() {
        if let EngineEvent::State { state, .. } = e {
            states.push(state);
        }
    }
    let p = states
        .iter()
        .position(|s| *s == DownloadState::Paused)
        .expect("paused");
    assert_eq!(states[p + 1], DownloadState::Queued, "{states:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_with_delete_removes_part_file() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        12 * MB,
        Faults {
            bytes_per_sec: Some(MB / 2),
            ..Default::default()
        },
    )
    .await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_bytes(m.store(), &id).await;
    m.cancel(&id, true).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Cancelled, 10).await;
    let leftovers: Vec<_> = std::fs::read_dir(d.path()).unwrap().collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_without_delete_still_removes_part_file() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        12 * MB,
        Faults {
            bytes_per_sec: Some(MB / 2),
            ..Default::default()
        },
    )
    .await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_bytes(m.store(), &id).await;
    m.cancel(&id, false).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Cancelled, 10).await;
    let leftovers: Vec<_> = std::fs::read_dir(d.path()).unwrap().collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn remove_without_delete_still_removes_part_file_of_paused() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        12 * MB,
        Faults {
            bytes_per_sec: Some(MB / 2),
            ..Default::default()
        },
    )
    .await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_bytes(m.store(), &id).await;
    m.pause(&id).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Paused, 10).await;
    assert!(std::fs::read_dir(d.path()).unwrap().next().is_some());
    m.remove(&id, false).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while m.store().get_download(&id).await.unwrap().is_some() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let leftovers: Vec<_> = std::fs::read_dir(d.path()).unwrap().collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn remove_deletes_record_and_file() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let mut rx = m.subscribe();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed, 10).await;
    m.remove(&id, true).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while m.store().get_download(&id).await.unwrap().is_some() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!final_path(&r).exists());
    let mut removed = false;
    while let Ok(e) = rx.try_recv() {
        removed |= matches!(e, EngineEvent::Removed { id: ref i } if *i == id);
    }
    assert!(removed);
}

#[tokio::test(flavor = "multi_thread")]
async fn same_url_twice_both_complete() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        3 * MB,
        Faults {
            bytes_per_sec: Some(4 * MB),
            ..Default::default()
        },
    )
    .await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let a = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let b = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let ra = wait_state(m.store(), &a, DownloadState::Completed, 30).await;
    let rb = wait_state(m.store(), &b, DownloadState::Completed, 30).await;
    let mut names = vec![ra.filename.clone(), rb.filename.clone()];
    names.sort();
    assert_eq!(names, vec!["file".to_string(), "file (1)".to_string()]);
    assert_eq!(std::fs::read(final_path(&ra)).unwrap(), body(3 * MB));
    assert_eq!(std::fs::read(final_path(&rb)).unwrap(), body(3 * MB));
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_then_resume_retries() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        1000,
        Faults {
            fail_status: Some(404),
            fail_times: 1,
            ..Default::default()
        },
    )
    .await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Failed, 10).await;
    assert_eq!(r.error_kind.as_deref(), Some("http_404"));
    m.resume(&id).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Completed, 10).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_then_restart_resumes() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("odm.db");
    let s = TestServer::start(
        12 * MB,
        Faults {
            bytes_per_sec: Some(MB / 2),
            ..Default::default()
        },
    )
    .await;
    let store = Store::open(&db).unwrap();
    let m = ManagerHandle::start(cfg(d.path()), store.clone())
        .await
        .unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_bytes(&store, &id).await;
    let t = Instant::now();
    m.shutdown().await;
    assert!(t.elapsed() < Duration::from_secs(4));
    assert_eq!(
        store.get_download(&id).await.unwrap().unwrap().state,
        DownloadState::Queued
    );
    assert!(
        m.pause(&id).await.is_err(),
        "commands after shutdown must error"
    );

    let m2 = ManagerHandle::start(cfg(d.path()), store.clone())
        .await
        .unwrap();
    let r = wait_state(m2.store(), &id, DownloadState::Completed, 60).await;
    assert_eq!(std::fs::read(final_path(&r)).unwrap(), body(12 * MB));
}

#[tokio::test(flavor = "multi_thread")]
async fn start_paused_and_bad_urls() {
    let d = tempfile::tempdir().unwrap();
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let mut r = AddRequest::new("http://127.0.0.1:9/never");
    r.start_paused = true;
    let id = m.add(r).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        m.store().get_download(&id).await.unwrap().unwrap().state,
        DownloadState::Paused
    );
    assert!(m.add(AddRequest::new("ftp://x/y")).await.is_err());
    assert!(m.add(AddRequest::new("javascript:alert(1)")).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn url_scheme_check_ignores_case() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let url = s.url("/file").replacen("http://", "HTTP://", 1);
    assert!(url.starts_with("HTTP://"), "{url}");
    let id = m.add(AddRequest::new(url)).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Completed, 10).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn set_config_applies_to_new_downloads() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = ManagerHandle::start(cfg(a.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    m.set_config(cfg(b.path())).await.unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed, 10).await;
    assert!(
        Path::new(&r.save_dir).starts_with(b.path()),
        "{}",
        r.save_dir
    );
    assert!(final_path(&r).exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn queries_report_bytes_and_active_count() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(2 * MB, Faults::default()).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    assert_eq!(m.active_count().await.unwrap(), 0);
    let mut req = AddRequest::new(s.url("/file"));
    req.headers = vec![("Cookie".into(), "secret=1".into())];
    let id = m.add(req).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Completed, 20).await;
    let (rec, bytes) = m.get(&id).await.unwrap().unwrap();
    assert_eq!((rec.size, bytes), (Some(2 * MB), 2 * MB));
    assert_eq!(m.list().await.unwrap().len(), 1);
    assert!(!m.chunks(&id).await.unwrap().is_empty());
    assert_eq!(
        m.header_names(&id).await.unwrap(),
        vec!["Cookie".to_string()]
    );
    assert!(m.get("nope").await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn auto_placement_uses_the_probe_mime_type() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        1000,
        Faults {
            content_type: Some("video/mp4".into()),
            ..Default::default()
        },
    )
    .await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();

    // No category and no folder given: placed by MIME type after the probe.
    let auto = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let r = wait_state(m.store(), &auto, DownloadState::Completed, 10).await;
    assert_eq!(r.category, "video");
    assert_eq!(Path::new(&r.save_dir), d.path().join("Videos"));
    assert!(final_path(&r).exists());

    // A folder chosen by the user is never changed.
    let mut req = AddRequest::new(s.url("/file"));
    req.save_dir = Some(d.path().join("mine"));
    let fixed = m.add(req).await.unwrap();
    let r = wait_state(m.store(), &fixed, DownloadState::Completed, 10).await;
    assert_eq!(Path::new(&r.save_dir), d.path().join("mine"));
}

#[tokio::test(flavor = "multi_thread")]
async fn state_counts_and_active_count_agree() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(2 * MB, Faults::default()).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    for _ in 0..2 {
        let mut req = AddRequest::new(s.url("/file"));
        req.start_paused = true;
        m.add(req).await.unwrap();
    }
    assert_eq!(m.state_counts().await.unwrap()[&DownloadState::Paused], 2);
    assert_eq!(m.active_count().await.unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_url_resumes_from_partial_data() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        8 * MB,
        Faults {
            bytes_per_sec: Some(2 * MB),
            ..Default::default()
        },
    )
    .await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let w: u64 = m.chunks(&id).await.unwrap().iter().map(|c| c.written).sum();
        if w >= MB {
            break;
        }
        assert!(Instant::now() < deadline, "no 1 MB of progress");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    m.pause(&id).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Paused, 20).await;
    let (_, paused_bytes) = m.get(&id).await.unwrap().unwrap();
    assert!(paused_bytes >= MB);

    m.refresh_url(&id, s.url("/file?token=2"), None)
        .await
        .unwrap();
    let mut rx = m.subscribe();
    m.resume(&id).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed, 60).await;
    assert_eq!(std::fs::read(final_path(&r)).unwrap(), body(8 * MB));
    assert!(r.url.ends_with("?token=2"), "{}", r.url);

    let first = loop {
        match rx.try_recv() {
            Ok(EngineEvent::Progress(p)) if p.id == id => break p,
            Ok(_) => {}
            Err(e) => panic!("no progress event after resume: {e:?}"),
        }
    };
    assert!(
        first.bytes >= paused_bytes,
        "restarted: {} < {paused_bytes}",
        first.bytes
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_url_refuses_active_and_bad_scheme() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        12 * MB,
        Faults {
            bytes_per_sec: Some(MB / 2),
            ..Default::default()
        },
    )
    .await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_bytes(m.store(), &id).await;
    assert!(m.refresh_url(&id, s.url("/file?x=1"), None).await.is_err());

    m.pause(&id).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Paused, 20).await;
    assert!(m
        .refresh_url(&id, "ftp://x/file".into(), None)
        .await
        .is_err());
    m.cancel(&id, true).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn set_target_renames_and_moves_before_start() {
    let d = tempfile::tempdir().unwrap();
    let d2 = tempfile::tempdir().unwrap();
    let s = TestServer::start(MB, Faults::default()).await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let id = m
        .add(AddRequest {
            start_paused: true,
            ..AddRequest::new(s.url("/file"))
        })
        .await
        .unwrap();
    m.set_target(
        &id,
        Some("new name.zip".into()),
        Some(d2.path().to_path_buf()),
    )
    .await
    .unwrap();
    m.resume(&id).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed, 20).await;
    assert_eq!(final_path(&r), d2.path().join("new name.zip"));
    assert_eq!(std::fs::read(final_path(&r)).unwrap(), body(MB));
    m.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn set_target_refuses_after_bytes_were_written() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        12 * MB,
        Faults {
            bytes_per_sec: Some(MB / 2),
            ..Default::default()
        },
    )
    .await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    wait_bytes(m.store(), &id).await;
    let err = m
        .set_target(&id, Some("x.bin".into()), None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("already started"), "{err}");

    m.pause(&id).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Paused, 20).await;
    assert!(m.set_target(&id, Some("x.bin".into()), None).await.is_err());
    m.cancel(&id, true).await.unwrap();
    m.shutdown().await;
}
