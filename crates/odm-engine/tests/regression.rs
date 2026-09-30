use odm_core::DownloadState;
use odm_engine::download::part_path;
use odm_engine::{AddRequest, EngineConfig, ManagerHandle};
use odm_store::Store;
use odm_testserver::{body, Faults, TestServer};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::{Duration, Instant};

const MB: u64 = 1 << 20;

fn cfg(dir: &Path) -> EngineConfig {
    EngineConfig {
        download_dir: dir.to_path_buf(),
        persist_interval: Duration::from_millis(200),
        ..Default::default()
    }
}

/// Regression: the old engine deadlocked when controllers + chunks exceeded the pool.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn many_downloads_many_connections_no_deadlock() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        8 * MB,
        Faults {
            bytes_per_sec: Some(MB),
            ..Default::default()
        },
    )
    .await;
    let mut c = cfg(d.path());
    c.max_concurrent = 10;
    c.max_connections = 32;
    let m = ManagerHandle::start(c, Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let mut ids = Vec::new();
    for _ in 0..32 {
        ids.push(m.add(AddRequest::new(s.url("/file"))).await.unwrap());
    }
    let res = tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let all = m.store().list_downloads().await.unwrap();
            if all.iter().all(|r| r.state == DownloadState::Completed) {
                break all;
            }
            assert!(
                all.iter().all(|r| r.state != DownloadState::Failed),
                "a download failed"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("deadlock: downloads did not finish in 120 s");
    let want = Sha256::digest(body(8 * MB)).to_vec();
    for r in res {
        let got = Sha256::digest(std::fs::read(Path::new(&r.save_dir).join(&r.filename)).unwrap())
            .to_vec();
        assert_eq!(got, want, "{}", r.filename);
    }
    let peak = s.max_concurrent();
    println!("observed peak concurrent connections: {peak}");
    assert!(
        peak > 32,
        "downloads did not overlap enough: peak {peak} connections (need > 32, the old pool size)"
    );
    m.shutdown().await;
}

/// Regression: kill the process mid-download (no graceful persist), restart, compare hashes.
#[test]
fn crash_mid_download_then_resume_is_identical() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("odm.db");
    let store = Store::open(&db).unwrap();

    // Server lives in its own runtime so it survives the "crash".
    let server_rt = tokio::runtime::Runtime::new().unwrap();
    let s = server_rt.block_on(TestServer::start(
        8 * MB,
        Faults {
            bytes_per_sec: Some(MB),
            ..Default::default()
        },
    ));
    let url = s.url("/file");

    // Runtime 1: start downloading, then drop it abruptly.
    let rt1 = tokio::runtime::Runtime::new().unwrap();
    let id = rt1.block_on(async {
        let m = ManagerHandle::start(cfg(d.path()), store.clone())
            .await
            .unwrap();
        let id = m.add(AddRequest::new(url.clone())).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let w: u64 = store
                .get_chunks(&id)
                .await
                .unwrap()
                .iter()
                .map(|c| c.written)
                .sum();
            if w > MB {
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        id
    });
    rt1.shutdown_background(); // abrupt: no final persist, no state change

    // Runtime 2: fresh manager over the same DB must finish with identical bytes.
    let rt2 = tokio::runtime::Runtime::new().unwrap();
    rt2.block_on(async {
        let m = ManagerHandle::start(cfg(d.path()), store.clone())
            .await
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let r = loop {
            let r = store.get_download(&id).await.unwrap().unwrap();
            if r.state == DownloadState::Completed {
                break r;
            }
            assert!(r.state != DownloadState::Failed, "{:?}", r.error_msg);
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        let got = Sha256::digest(std::fs::read(Path::new(&r.save_dir).join(&r.filename)).unwrap())
            .to_vec();
        assert_eq!(got, Sha256::digest(body(8 * MB)).to_vec());
        m.shutdown().await;
    });
    drop(s);
}

/// Regression: the old allocator produced a 1-byte file.
#[tokio::test(flavor = "multi_thread")]
async fn part_file_is_preallocated_to_full_size() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        8 * MB,
        Faults {
            bytes_per_sec: Some(MB),
            ..Default::default()
        },
    )
    .await;
    let m = ManagerHandle::start(cfg(d.path()), Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let id = m.add(AddRequest::new(s.url("/file"))).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let r = m.store().get_download(&id).await.unwrap().unwrap();
        if r.state == DownloadState::Downloading {
            let part = part_path(&r);
            if let Ok(meta) = std::fs::metadata(&part) {
                assert_eq!(meta.len(), 8 * MB);
                break;
            }
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    m.cancel(&id, true).await.unwrap();
}
