use odm_core::ErrorKind;
use odm_engine::download::{build_client, part_path, run_download, DownloadEnv, Outcome};
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
        ..Default::default()
    };
    let (events, _) = tokio::sync::broadcast::channel(1024);
    let env = DownloadEnv {
        store: Store::open_in_memory().unwrap(),
        client: build_client(&config),
        limiter: Arc::new(SpeedLimiter::new(0)),
        events,
        config,
    };
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
    std::fs::read_dir(dir).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".odm-part"))
}

#[tokio::test]
async fn multi_connection_download_matches() {
    let f = fx();
    let s = TestServer::start(8 * MB, Faults { bytes_per_sec: Some(8 * MB), ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(sha(&p), Sha256::digest(body(8 * MB)).to_vec());
    assert!(s.max_concurrent() >= 2, "used {} connections", s.max_concurrent());
    assert!(no_parts(f.dir.path()));
}

#[tokio::test]
async fn no_range_server_uses_single_connection() {
    let f = fx();
    let s = TestServer::start(3 * MB, Faults { no_ranges: true, ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 8).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(3 * MB));
    assert_eq!(f.env.store.get_chunks(&id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn unknown_size_download_completes() {
    let f = fx();
    let s = TestServer::start(500_000, Faults { no_ranges: true, no_length: true, ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 8).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(500_000));
    assert_eq!(f.env.store.get_download(&id).await.unwrap().unwrap().size, Some(500_000));
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
    let s = TestServer::start(4 * MB, Faults { wrong_range_start: true, ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(4 * MB));
}

#[tokio::test]
async fn dropped_connections_are_retried() {
    let f = fx();
    let s = TestServer::start(4 * MB, Faults { drop_after: Some(200_000), drop_times: 4, ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(4 * MB));
}

#[tokio::test]
async fn persistent_transient_error_fails_after_attempts() {
    let mut f = fx();
    f.env.config.max_attempts = 2;
    let s = TestServer::start(1000, Faults { fail_status: Some(503), fail_times: 1000, ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 4).await;
    match run_download(&f.env, &id, CancellationToken::new()).await {
        Outcome::Failed(e) => assert_eq!(e.kind, ErrorKind::HttpTransient(503)),
        o => panic!("{o:?}"),
    }
}

#[tokio::test]
async fn expired_link_fails_without_retry() {
    let f = fx();
    let s = TestServer::start(2 * MB, Faults { forbid_after: Some(1), ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 2).await;
    match run_download(&f.env, &id, CancellationToken::new()).await {
        Outcome::Failed(e) => assert_eq!(e.kind, ErrorKind::LinkExpired),
        o => panic!("{o:?}"),
    }
}

#[tokio::test]
async fn cancel_then_resume_produces_identical_file() {
    let f = fx();
    let s = TestServer::start(6 * MB, Faults { bytes_per_sec: Some(2 * MB), ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 3).await;
    let tok = CancellationToken::new();
    let t2 = tok.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(700)).await;
        t2.cancel();
    });
    assert!(matches!(run_download(&f.env, &id, tok).await, Outcome::Stopped));
    let saved: u64 = f.env.store.get_chunks(&id).await.unwrap().iter().map(|c| c.written).sum();
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
        .replace_chunks(&id, vec![
            ChunkRecord { idx: 0, start: 0, end: Some(6 * MB - 1), written: 6 * MB },
            ChunkRecord { idx: 1, start: 6 * MB, end: Some(12 * MB - 1), written: 0 },
        ])
        .await
        .unwrap();

    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(sha(&p), Sha256::digest(body(12 * MB)).to_vec());
    assert!(f.env.store.get_chunks(&id).await.unwrap().len() >= 3, "free connection was not used");
}

#[tokio::test]
async fn changed_server_file_restarts_from_zero() {
    let f = fx();
    let s = TestServer::start(4 * MB, Faults { etag_changes_after: Some(2), ..Default::default() }).await;
    let id = add(&f, &s.url("/file"), 4).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(std::fs::read(p).unwrap(), body(4 * MB));
    assert_eq!(f.env.store.get_download(&id).await.unwrap().unwrap().etag.as_deref(), Some("\"v2\""));
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
        Outcome::Failed(e) => assert!(matches!(e.kind, ErrorKind::Io | ErrorKind::PermissionDenied), "{e}"),
        o => panic!("{o:?}"),
    }
}

#[tokio::test]
async fn name_collision_and_disposition() {
    let f = fx();
    let s = TestServer::start(1000, Faults { filename: Some("report.pdf".into()), ..Default::default() }).await;
    std::fs::write(f.dir.path().join("report.pdf"), b"old").unwrap();
    let id = add(&f, &s.url("/file"), 2).await;
    let p = done(run_download(&f.env, &id, CancellationToken::new()).await);
    assert_eq!(p.file_name().unwrap(), "report (1).pdf");
    assert_eq!(std::fs::read(f.dir.path().join("report.pdf")).unwrap(), b"old");
    assert_eq!(f.env.store.get_download(&id).await.unwrap().unwrap().filename, "report (1).pdf");
}

#[tokio::test]
async fn checksum_verified() {
    let f = fx();
    let s = TestServer::start(10_000, Faults::default()).await;
    let good = format!("sha256:{}", hex(&Sha256::digest(body(10_000))));
    for (sum, ok) in [(good, true), ("sha256:00".to_string(), false)] {
        let id = add(&f, &s.url("/file"), 1).await;
        f.env.store.call({
            let id = id.clone();
            move |c| c.execute("UPDATE downloads SET checksum=?2 WHERE id=?1", [&id, &sum]).map(|_| ())
        }).await.unwrap();
        match (run_download(&f.env, &id, CancellationToken::new()).await, ok) {
            (Outcome::Completed { .. }, true) => {}
            (Outcome::Failed(e), false) => assert_eq!(e.kind, ErrorKind::ChecksumMismatch),
            (o, _) => panic!("{o:?}"),
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
