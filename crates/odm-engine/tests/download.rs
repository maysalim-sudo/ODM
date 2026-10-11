use odm_core::ErrorKind;
use odm_engine::download::{part_path, run_download, DownloadEnv, Outcome};
use odm_engine::{EngineConfig, SpeedLimiter};
use odm_store::{ChunkRecord, DownloadRecord, Store};
use odm_testserver::{body, Faults, TestServer};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const MB: u64 = 1 << 20;

struct Fx {
    env: DownloadEnv,
    dir: tempfile::TempDir,
}

fn fx() -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let config = EngineConfig {
        download_dir: dir.path().to_path_buf(),
        progress_interval: Duration::from_millis(50),
        persist_interval: Duration::from_millis(200),
        max_attempts: 5,
        // Hermetic behind a system proxy: the test servers are on this PC.
        proxy: odm_engine::config::ProxyConfig::Direct,
        ..Default::default()
    };
    let (events, _) = tokio::sync::broadcast::channel(1024);
    let env = DownloadEnv::new(
        Store::open_in_memory().unwrap(),
        config,
        Arc::new(SpeedLimiter::new(0)),
        events,
    );
    Fx { env, dir }
}

async fn add(f: &Fx, url: &str, conns: u32) -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let mut r = DownloadRecord::new_file(&id, url, "file.bin", f.dir.path().to_str().unwrap());
    r.max_connections = conns;
    f.env.store.insert_download(r).await.unwrap();
    id
}

fn sha(p: &Path) -> Vec<u8> {
    Sha256::digest(std::fs::read(p).unwrap()).to_vec()
}

fn done(o: Outcome) -> PathBuf {
    match o {
        Outcome::Completed { path } => path,
        other => panic!("expected Completed, got {other:?}"),
    }
}

fn no_parts(dir: &Path) -> bool {
    std::fs::read_dir(dir).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".odm-part")
    })
}

#[tokio::test]
async fn multi_connection_download_matches() {
    let f = fx();
    let s = TestServer::start(
        8 * MB,
        Faults {
            bytes_per_sec: Some(8 * MB),
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(sha(&p), Sha256::digest(body(8 * MB)).to_vec());
    assert!(
        s.max_concurrent() >= 2,
        "used {} connections",
        s.max_concurrent()
    );
    assert!(no_parts(f.dir.path()));
}

#[tokio::test]
async fn no_range_server_uses_single_connection() {
    let f = fx();
    let s = TestServer::start(
        3 * MB,
        Faults {
            no_ranges: true,
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 8).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(3 * MB));
    assert_eq!(f.env.store.get_chunks(&id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn unknown_size_download_completes() {
    let f = fx();
    let s = TestServer::start(
        500_000,
        Faults {
            no_ranges: true,
            no_length: true,
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 8).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(500_000));
    assert_eq!(
        f.env.store.get_download(&id).await.unwrap().unwrap().size,
        Some(500_000)
    );
}

#[tokio::test]
async fn zero_byte_download_completes() {
    let f = fx();
    let s = TestServer::start(0, Faults::default()).await;
    let id = add(&f, &s.url("/file"), 8).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::metadata(p).unwrap().len(), 0);
}

#[tokio::test]
async fn range_ignored_falls_back_to_single_connection() {
    // Probe sees 206 (so ranges look supported) but every range reply has a wrong start.
    let f = fx();
    let s = TestServer::start(
        4 * MB,
        Faults {
            wrong_range_start: true,
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(4 * MB));
}

#[tokio::test]
async fn dropped_connections_are_retried() {
    let f = fx();
    let s = TestServer::start(
        4 * MB,
        Faults {
            drop_after: Some(200_000),
            drop_times: 4,
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(4 * MB));
}

#[tokio::test]
async fn persistent_transient_error_fails_after_attempts() {
    let mut f = fx();
    f.env.config.max_attempts = 2;
    let s = TestServer::start(
        1000,
        Faults {
            fail_status: Some(503),
            fail_times: 1000,
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 4).await;
    match run_download(&f.env, &id, CancellationToken::new()).await {
        Outcome::Failed(e) => assert_eq!(e.kind, ErrorKind::HttpTransient(503)),
        o => panic!("{o:?}"),
    }
}

#[tokio::test]
async fn connection_limited_server_still_completes() {
    // The server allows 3 parallel bodies; the download asks for 8 connections.
    let f = fx();
    let s = TestServer::start(
        12 * MB,
        Faults {
            max_parallel: Some(3),
            bytes_per_sec: Some(2 * MB),
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 8).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(sha(&p), Sha256::digest(body(12 * MB)).to_vec());
    assert!(
        s.max_concurrent() <= 3,
        "server saw {} parallel bodies",
        s.max_concurrent()
    );
    // 8 connections against a limit of 3: at most 5 refusals at start-up, plus a
    // small allowance for replacements refused before a slot was released.
    assert!(s.refused() <= 12, "server refused {} requests", s.refused());
}

#[tokio::test]
async fn refused_connections_do_not_use_up_the_retry_budget() {
    // With max_attempts = 2 the old engine failed here within seconds.
    let mut f = fx();
    f.env.config.max_attempts = 2;
    let s = TestServer::start(
        8 * MB,
        Faults {
            max_parallel: Some(1),
            bytes_per_sec: Some(4 * MB),
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 8).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(8 * MB));
}

#[tokio::test]
async fn expired_link_fails_without_retry() {
    let f = fx();
    let s = TestServer::start(
        2 * MB,
        Faults {
            forbid_after: Some(1),
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 2).await;
    match run_download(&f.env, &id, CancellationToken::new()).await {
        Outcome::Failed(e) => assert_eq!(e.kind, ErrorKind::LinkExpired),
        o => panic!("{o:?}"),
    }
}

#[tokio::test]
async fn cancel_then_resume_produces_identical_file() {
    let f = fx();
    let s = TestServer::start(
        6 * MB,
        Faults {
            bytes_per_sec: Some(2 * MB),
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 3).await;
    let tok = CancellationToken::new();
    let t2 = tok.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(700)).await;
        t2.cancel();
    });
    assert!(matches!(
        run_download(&f.env, &id, tok).await,
        Outcome::Stopped
    ));
    let saved: u64 = f
        .env
        .store
        .get_chunks(&id)
        .await
        .unwrap()
        .iter()
        .map(|c| c.written)
        .sum();
    assert!(saved > 0 && saved < 6 * MB, "{saved}");

    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(sha(&p), Sha256::digest(body(6 * MB)).to_vec());
}

#[tokio::test]
async fn resume_fills_free_connections_by_splitting() {
    let f = fx();
    let s = TestServer::start(12 * MB, Faults::default()).await;
    let id = add(&f, &s.url("/file"), 2).await;
    let mut rec = f.env.store.get_download(&id).await.unwrap().unwrap();
    rec.size = Some(12 * MB);
    rec.etag = Some("\"v1\"".into());
    f.env.store.delete_download(&id).await.unwrap();
    f.env.store.insert_download(rec.clone()).await.unwrap();
    // First half already on disk and recorded as done; second half untouched.
    let part = part_path(&rec);
    let mut data = body(12 * MB);
    data[6 * MB as usize..].fill(0);
    std::fs::write(&part, &data).unwrap();
    f.env
        .store
        .replace_chunks(
            &id,
            vec![
                ChunkRecord {
                    idx: 0,
                    start: 0,
                    end: Some(6 * MB - 1),
                    written: 6 * MB,
                },
                ChunkRecord {
                    idx: 1,
                    start: 6 * MB,
                    end: Some(12 * MB - 1),
                    written: 0,
                },
            ],
        )
        .await
        .unwrap();

    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(sha(&p), Sha256::digest(body(12 * MB)).to_vec());
    assert!(
        f.env.store.get_chunks(&id).await.unwrap().len() >= 3,
        "free connection was not used"
    );
}

#[tokio::test]
async fn changed_server_file_restarts_from_zero() {
    let f = fx();
    let s = TestServer::start(
        4 * MB,
        Faults {
            etag_changes_after: Some(2),
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(4 * MB));
    assert_eq!(
        f.env
            .store
            .get_download(&id)
            .await
            .unwrap()
            .unwrap()
            .etag
            .as_deref(),
        Some("\"v2\"")
    );
}

#[tokio::test]
async fn save_dir_is_a_file_fails_cleanly() {
    let f = fx();
    let s = TestServer::start(1000, Faults::default()).await;
    let blocker = f.dir.path().join("blocker");
    std::fs::write(&blocker, b"x").unwrap();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let r = DownloadRecord::new_file(&id, &s.url("/file"), "a.bin", blocker.to_str().unwrap());
    f.env.store.insert_download(r).await.unwrap();
    match run_download(&f.env, &id, CancellationToken::new()).await {
        Outcome::Failed(e) => assert!(
            matches!(e.kind, ErrorKind::Io | ErrorKind::PermissionDenied),
            "{e}"
        ),
        o => panic!("{o:?}"),
    }
}

#[tokio::test]
async fn name_collision_and_disposition() {
    let f = fx();
    let s = TestServer::start(
        1000,
        Faults {
            filename: Some("report.pdf".into()),
            ..Default::default()
        },
    )
    .await;
    std::fs::write(f.dir.path().join("report.pdf"), b"old").unwrap();
    let id = add(&f, &s.url("/file"), 2).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(p.file_name().unwrap(), "report (1).pdf");
    assert_eq!(
        std::fs::read(f.dir.path().join("report.pdf")).unwrap(),
        b"old"
    );
    assert_eq!(
        f.env
            .store
            .get_download(&id)
            .await
            .unwrap()
            .unwrap()
            .filename,
        "report (1).pdf"
    );
}

#[tokio::test]
async fn checksum_verified() {
    let f = fx();
    let s = TestServer::start(10_000, Faults::default()).await;
    let good = format!("sha256:{}", hex(&Sha256::digest(body(10_000))));
    for (sum, ok) in [(good, true), ("sha256:00".to_string(), false)] {
        let id = add(&f, &s.url("/file"), 1).await;
        f.env
            .store
            .call({
                let id = id.clone();
                move |c| {
                    c.execute("UPDATE downloads SET checksum=?2 WHERE id=?1", [&id, &sum])
                        .map(|_| ())
                }
            })
            .await
            .unwrap();
        match (
            run_download(&f.env, &id, CancellationToken::new()).await,
            ok,
        ) {
            (Outcome::Completed { .. }, true) => {}
            (Outcome::Failed(e), false) => assert_eq!(e.kind, ErrorKind::ChecksumMismatch),
            (o, _) => panic!("{o:?}"),
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[tokio::test]
async fn resume_with_deleted_part_file_redownloads() {
    let f = fx();
    let s = TestServer::start(
        6 * MB,
        Faults {
            bytes_per_sec: Some(2 * MB),
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 3).await;
    let tok = CancellationToken::new();
    let t2 = tok.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(700)).await;
        t2.cancel();
    });
    assert!(matches!(
        run_download(&f.env, &id, tok).await,
        Outcome::Stopped
    ));
    let saved: u64 = f
        .env
        .store
        .get_chunks(&id)
        .await
        .unwrap()
        .iter()
        .map(|c| c.written)
        .sum();
    assert!(saved > 0, "{saved}");
    let rec = f.env.store.get_download(&id).await.unwrap().unwrap();
    std::fs::remove_file(part_path(&rec)).unwrap();

    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(sha(&p), Sha256::digest(body(6 * MB)).to_vec());
}

#[tokio::test]
async fn no_range_server_survives_repeated_drops() {
    let f = fx();
    let s = TestServer::start(
        2 * MB,
        Faults {
            no_ranges: true,
            drop_after: Some(300_000),
            drop_times: 3,
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(2 * MB));
}

#[tokio::test]
async fn cookies_are_not_replayed_to_a_cross_origin_redirect_target() {
    // A (original URL) redirects to B on another origin. reqwest strips Cookie
    // on that hop; the engine must not defeat that by requesting B directly.
    let f = fx();
    let b = TestServer::start(3 * MB, Faults::default()).await;
    let a = TestServer::start(
        0,
        Faults {
            redirect_to: Some(b.url("/file")),
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &a.url("/file"), 4).await;
    f.env
        .store
        .set_headers(&id, vec![("Cookie".into(), "session=secret".into())])
        .await
        .unwrap();
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(3 * MB));
    assert!(a.cookie_requests() >= 1, "origin A never saw the cookie");
    assert_eq!(b.cookie_requests(), 0, "cookie leaked to cross-origin B");
    // Every chunk request starts at the original URL (probe + >= 1 worker).
    assert!(a.requests.load(std::sync::atomic::Ordering::SeqCst) >= 2);
}

#[tokio::test]
async fn range_ignored_with_unchanged_etag_falls_back_to_single_connection() {
    // The probe gets a 206, but every other Range gets a 200 full body with the
    // SAME ETag: that is RangeIgnored (single-connection fallback), not FileChanged.
    let f = fx();
    let s = TestServer::start(
        4 * MB,
        Faults {
            only_probe_range: true,
            ..Default::default()
        },
    )
    .await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(4 * MB));
    assert_eq!(f.env.store.get_chunks(&id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn final_progress_event_reports_every_chunk_done() {
    use odm_engine::{ChunkViewState, EngineEvent};
    let f = fx();
    let mut rx = f.env.events.subscribe();
    let s = TestServer::start(4 * MB, Faults::default()).await;
    let id = add(&f, &s.url("/file"), 4).await;
    done(run_download(&f.env, &id, CancellationToken::new()).await);
    let mut last = None;
    while let Ok(ev) = rx.try_recv() {
        if let EngineEvent::Progress(p) = ev {
            last = Some(p);
        }
    }
    let p = last.expect("no progress event");
    assert!(p.chunks.len() > 1);
    assert!(p.chunks.iter().all(|c| c.state == ChunkViewState::Done));
    let mut idxs: Vec<u32> = p.chunks.iter().map(|c| c.idx).collect();
    idxs.sort_unstable();
    idxs.dedup();
    assert_eq!(idxs.len(), p.chunks.len(), "chunk idx must be unique");
}

#[cfg(windows)]
#[tokio::test]
async fn completed_file_gets_mark_of_the_web() {
    let f = fx();
    let s = TestServer::start(MB, Faults::default()).await;
    let id = add(&f, &s.url("/file?token=secret"), 1).await;
    let mut rec = f.env.store.get_download(&id).await.unwrap().unwrap();
    rec.page_url = Some("https://site.example/page?q=1".into());
    f.env.store.delete_download(&id).await.unwrap();
    f.env.store.insert_download(rec).await.unwrap();
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    let zone = std::fs::read_to_string(format!("{}:Zone.Identifier", p.display())).unwrap();
    assert!(zone.contains("ZoneId=3"), "{zone}");
    assert!(zone.contains("HostUrl=http://127.0.0.1:"), "{zone}");
    assert!(
        zone.contains("ReferrerUrl=https://site.example/page"),
        "{zone}"
    );
    assert!(!zone.contains("secret") && !zone.contains("q=1"), "{zone}");
}
