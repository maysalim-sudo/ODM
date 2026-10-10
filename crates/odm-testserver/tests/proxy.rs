//! A manual proxy is really used: the download URL names a host that only the test proxy
//! knows, so it can only complete through the proxy.
use odm_core::DownloadState;
use odm_engine::config::ProxyConfig;
use odm_engine::{AddRequest, EngineConfig, ManagerHandle};
use odm_store::Store;
use odm_testserver::proxy::TestProxy;
use odm_testserver::{Faults, TestServer};
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// `.test` is reserved, so this name never resolves outside the proxy.
const FAKE_HOST: &str = "files.odm.test";

fn cfg(dir: &Path, proxy: ProxyConfig, ua: Option<&str>) -> EngineConfig {
    EngineConfig {
        download_dir: dir.to_path_buf(),
        progress_interval: Duration::from_millis(50),
        persist_interval: Duration::from_millis(200),
        proxy,
        custom_user_agent: ua.map(str::to_string),
        ..EngineConfig::default()
    }
}

async fn download(cfg: EngineConfig, headers: Vec<(String, String)>) -> DownloadState {
    let m = ManagerHandle::start(cfg, Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let id = m
        .add(AddRequest {
            headers,
            ..AddRequest::new(format!("http://{FAKE_HOST}/file"))
        })
        .await
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let (r, _) = m.get(&id).await.unwrap().unwrap();
        if matches!(r.state, DownloadState::Completed | DownloadState::Failed) {
            return r.state;
        }
        assert!(Instant::now() < deadline, "timed out in {:?}", r.state);
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_manual_proxy_carries_the_download_with_the_custom_agent() {
    let server = TestServer::start(200_000, Faults::default()).await;
    let proxy = TestProxy::start(&server.base).await;
    let out = tempfile::tempdir().unwrap();
    let state = download(
        cfg(
            out.path(),
            ProxyConfig::Manual(proxy.url()),
            Some("Custom/1"),
        ),
        vec![],
    )
    .await;
    assert_eq!(state, DownloadState::Completed);
    assert_eq!(
        std::fs::read(out.path().join("file")).unwrap(),
        odm_testserver::body(200_000)
    );
    let seen = proxy.seen();
    assert!(!seen.is_empty());
    for s in &seen {
        assert_eq!(s.target, format!("http://{FAKE_HOST}/file"));
        assert_eq!(s.user_agent.as_deref(), Some("Custom/1"));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_browser_agent_wins_over_the_custom_one() {
    let server = TestServer::start(50_000, Faults::default()).await;
    let proxy = TestProxy::start(&server.base).await;
    let out = tempfile::tempdir().unwrap();
    let state = download(
        cfg(
            out.path(),
            ProxyConfig::Manual(proxy.url()),
            Some("Custom/1"),
        ),
        vec![("User-Agent".into(), "Browser/9".into())],
    )
    .await;
    assert_eq!(state, DownloadState::Completed);
    let seen = proxy.seen();
    assert!(!seen.is_empty());
    assert!(seen
        .iter()
        .all(|s| s.user_agent.as_deref() == Some("Browser/9")));
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_custom_agent_odm_sends_its_own() {
    let server = TestServer::start(10_000, Faults::default()).await;
    let proxy = TestProxy::start(&server.base).await;
    let out = tempfile::tempdir().unwrap();
    let state = download(
        cfg(out.path(), ProxyConfig::Manual(proxy.url()), None),
        vec![],
    )
    .await;
    assert_eq!(state, DownloadState::Completed);
    let own = format!("ODM/{}", env!("CARGO_PKG_VERSION"));
    assert!(proxy
        .seen()
        .iter()
        .all(|s| s.user_agent.as_deref() == Some(own.as_str())));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_proxy_tunnels_connect_requests() {
    let server = TestServer::start(1_000, Faults::default()).await;
    let proxy = TestProxy::start(&server.base).await;
    let mut s = tokio::net::TcpStream::connect(proxy.addr).await.unwrap();
    s.write_all(b"CONNECT secure.odm.test:443 HTTP/1.1\r\nHost: secure.odm.test:443\r\n\r\n")
        .await
        .unwrap();
    let mut buf = vec![0u8; 1024];
    let n = s.read(&mut buf).await.unwrap();
    assert!(String::from_utf8_lossy(&buf[..n]).starts_with("HTTP/1.1 200"));
    s.write_all(b"GET /file HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut resp = Vec::new();
    s.read_to_end(&mut resp).await.unwrap();
    assert!(String::from_utf8_lossy(&resp).starts_with("HTTP/1.1 200"));
    assert!(resp.ends_with(&odm_testserver::body(1_000)));
    assert_eq!(proxy.seen()[0].target, "secure.odm.test:443");
}

#[tokio::test(flavor = "multi_thread")]
async fn without_the_proxy_the_fake_host_cannot_be_reached() {
    let out = tempfile::tempdir().unwrap();
    let c = EngineConfig {
        max_attempts: 1,
        ..cfg(out.path(), ProxyConfig::Direct, None)
    };
    assert_eq!(download(c, vec![]).await, DownloadState::Failed);
}
