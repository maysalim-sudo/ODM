use odm_appcore::api::{self, AddArgs};
use odm_appcore::dto::UiEvent;
use odm_appcore::Settings;
use odm_core::DownloadState;
use odm_engine::{ChunkViewState, EngineConfig, ManagerHandle};
use odm_store::Store;
use odm_testserver::{Faults, TestServer};
use std::path::Path;
use std::time::{Duration, Instant};

fn cfg(dir: &Path) -> EngineConfig {
    EngineConfig {
        download_dir: dir.to_path_buf(),
        progress_interval: Duration::from_millis(50),
        persist_interval: Duration::from_millis(200),
        ..Default::default()
    }
}

async fn manager(dir: &Path) -> ManagerHandle {
    ManagerHandle::start(cfg(dir), Store::open_in_memory().unwrap())
        .await
        .unwrap()
}

async fn wait_completed(m: &ManagerHandle, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let (r, _) = m.get(id).await.unwrap().unwrap();
        if r.state == DownloadState::Completed {
            return;
        }
        assert!(r.state != DownloadState::Failed, "{:?}", r.error_msg);
        assert!(
            Instant::now() < deadline,
            "timed out in state {:?}",
            r.state
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[test]
fn parse_urls_filters_and_dedupes() {
    let text = "https://a.com/1.zip\n\n  http://b.com/2.zip  \nftp://c.com/3\nhttps://a.com/1.zip\nnot a url\nHTTPS://D.com/4\nhttps://\n";
    let p = api::parse_urls(text);
    assert_eq!(
        p.valid,
        vec![
            "https://a.com/1.zip",
            "http://b.com/2.zip",
            "HTTPS://D.com/4"
        ]
    );
    assert_eq!(p.rejected, vec!["ftp://c.com/3", "not a url", "https://"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn add_list_and_detail_round_trip() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(2 << 20, Faults::default()).await;
    let m = manager(d.path()).await;
    let id = api::add(
        &m,
        AddArgs {
            url: s.url("/file"),
            filename: Some("  ".into()),
            category: Some(String::new()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    wait_completed(&m, &id).await;

    let list = api::list(&m).await.unwrap();
    assert_eq!(list.len(), 1);
    let dto = &list[0];
    assert_eq!(dto.filename, "file"); // blank filename was treated as "not given"
    assert_eq!((dto.size, dto.bytes), (Some(2 << 20), 2 << 20));
    assert_eq!(dto.state, DownloadState::Completed);

    let detail = api::detail(&m, &id).await.unwrap().unwrap();
    assert!(!detail.chunks.is_empty());
    assert!(detail
        .chunks
        .iter()
        .all(|c| c.state == ChunkViewState::Done));
    assert!(api::detail(&m, "nope").await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn detail_exposes_header_names_never_values() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = manager(d.path()).await;
    let mut req = odm_engine::AddRequest::new(s.url("/file"));
    req.headers = vec![("Cookie".into(), "session=TOPSECRET".into())];
    let id = m.add(req).await.unwrap();
    wait_completed(&m, &id).await;
    let detail = api::detail(&m, &id).await.unwrap().unwrap();
    assert_eq!(detail.header_names, vec!["Cookie".to_string()]);
    let json = serde_json::to_string(&detail).unwrap();
    assert!(!json.contains("TOPSECRET"), "{json}");
}

#[tokio::test(flavor = "multi_thread")]
async fn add_rejects_bad_urls_with_a_message() {
    let d = tempfile::tempdir().unwrap();
    let m = manager(d.path()).await;
    let err = api::add(
        &m,
        AddArgs {
            url: "ftp://x/y".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(err.contains("http"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn batch_adds_valid_urls_and_reports_the_rest() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = manager(d.path()).await;
    let text = format!("{}\nnope\n{}\n", s.url("/file"), s.url("/file"));
    let r = api::add_batch(
        &m,
        &text,
        Some(d.path().join("batch").to_string_lossy().into_owned()),
    )
    .await
    .unwrap();
    assert_eq!(r.added.len(), 1);
    assert_eq!(r.rejected, vec!["nope".to_string()]);
    wait_completed(&m, &r.added[0]).await;
    assert!(d.path().join("batch").join("file").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn suggest_reads_name_size_and_category_from_the_server() {
    let s = TestServer::start(
        5000,
        Faults {
            filename: Some("report.pdf".into()),
            ..Default::default()
        },
    )
    .await;
    let sug = api::suggest(&s.url("/file"), &EngineConfig::default())
        .await
        .unwrap();
    assert_eq!(sug.filename, "report.pdf");
    assert_eq!(sug.size, Some(5000));
    assert_eq!(sug.category, "document");
    assert!(sug.resumable);

    let s = TestServer::start(
        10,
        Faults {
            fail_status: Some(404),
            fail_times: 9,
            ..Default::default()
        },
    )
    .await;
    assert!(api::suggest(&s.url("/file"), &EngineConfig::default())
        .await
        .unwrap_err()
        .contains("http_404"));
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_settings_changes_where_new_downloads_go() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = manager(a.path()).await;
    let settings = Settings {
        download_dir: b.path().to_string_lossy().into_owned(),
        ..Default::default()
    };
    api::apply_settings(&m, &settings).await.unwrap();
    let id = api::add(
        &m,
        AddArgs {
            url: s.url("/file"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    wait_completed(&m, &id).await;
    assert!(b.path().join("file").exists());
}

#[test]
fn ui_event_json_shape_is_pinned() {
    let json = serde_json::to_string(&UiEvent::Removed { id: "x".into() }).unwrap();
    assert_eq!(json, r#"{"type":"removed","id":"x"}"#);
    let json = serde_json::to_string(&UiEvent::Resync).unwrap();
    assert_eq!(json, r#"{"type":"resync"}"#);
    let json = serde_json::to_string(&UiEvent::ProgressBatch { items: vec![] }).unwrap();
    assert_eq!(json, r#"{"type":"progress_batch","items":[]}"#);
}
