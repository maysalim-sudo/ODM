use odm_appcore::api::{self, AddArgs};
use odm_appcore::dto::UiEvent;
use odm_appcore::Settings;
use odm_core::DownloadState;
use odm_engine::{ChunkViewState, EngineConfig, ManagerHandle};
use odm_store::Store;
use odm_testserver::hls::{write_synthetic, HlsFaults, HlsServer};
use odm_testserver::{Faults, TestServer};
use std::path::Path;
use std::time::{Duration, Instant};

fn cfg(dir: &Path) -> EngineConfig {
    EngineConfig {
        download_dir: dir.to_path_buf(),
        progress_interval: Duration::from_millis(50),
        persist_interval: Duration::from_millis(200),
        // Hermetic behind a system proxy: the test servers are on this PC.
        proxy: odm_engine::config::ProxyConfig::Direct,
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
        false,
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
        .contains("404"));
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

#[tokio::test(flavor = "multi_thread")]
async fn save_settings_clamps_persists_and_applies() {
    let data = tempfile::tempdir().unwrap();
    let dl = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = manager(data.path()).await;
    let path = data.path().join("settings.json");

    let wanted = Settings {
        download_dir: dl.path().to_string_lossy().into_owned(),
        max_connections: 500,
        ..Default::default()
    };
    let (saved, warnings) = api::save_settings(&m, &path, wanted).await.unwrap();
    assert_eq!(saved.max_connections, 32);
    assert_eq!(warnings.len(), 1);
    assert_eq!(Settings::load(&path).0, saved);

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
    assert!(dl.path().join("file").exists());
}

#[test]
fn completed_download_only_matches_completed_upserts() {
    use odm_appcore::dto::{completed_download, to_dto};
    let mut r = odm_store::DownloadRecord::new_file("a", "https://e.com/f", "f", "C:/dl");
    let queued = UiEvent::Upsert {
        download: Box::new(to_dto(&r, 0)),
    };
    r.state = DownloadState::Completed;
    let done = UiEvent::Upsert {
        download: Box::new(to_dto(&r, 0)),
    };
    assert!(completed_download(&queued).is_none());
    assert_eq!(completed_download(&done).unwrap().id, "a");
    assert!(completed_download(&UiEvent::Resync).is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_url_api_resumes() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        8 << 20,
        Faults {
            bytes_per_sec: Some(1 << 20),
            ..Default::default()
        },
    )
    .await;
    let m = manager(d.path()).await;
    let id = api::add(
        &m,
        AddArgs {
            url: s.url("/file"),
            start_paused: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    api::refresh_url(&m, &id, &s.url("/file?token=2"))
        .await
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let (r, _) = m.get(&id).await.unwrap().unwrap();
        if matches!(
            r.state,
            DownloadState::Downloading | DownloadState::Completed
        ) {
            assert!(r.url.ends_with("?token=2"));
            break;
        }
        assert!(Instant::now() < deadline, "stuck in {:?}", r.state);
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    m.cancel(&id, true).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn resolve_offer_reuse_and_fresh() {
    use odm_appcore::offers::Offers;
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(
        8 << 20,
        Faults {
            bytes_per_sec: Some(1 << 20),
            ..Default::default()
        },
    )
    .await;
    let m = manager(d.path()).await;
    let existing = api::add(
        &m,
        AddArgs {
            url: s.url("/file"),
            filename: Some("a.bin".into()),
            start_paused: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let offers = Offers::default();
    let mut req = odm_engine::AddRequest::new(s.url("/file?token=2"));
    req.filename = Some("a.bin".into());
    req.start_paused = true;
    let a = offers.create(existing.clone(), "a.bin".into(), req.clone());
    let got = api::resolve_offer(&m, &offers, &a.offer_id, true)
        .await
        .unwrap();
    assert_eq!(got, existing);
    assert_eq!(m.list().await.unwrap().len(), 1);

    let b = offers.create(existing.clone(), "a.bin".into(), req);
    let fresh = api::resolve_offer(&m, &offers, &b.offer_id, false)
        .await
        .unwrap();
    assert_ne!(fresh, existing);
    assert_eq!(m.list().await.unwrap().len(), 2);

    let err = api::resolve_offer(&m, &offers, "nope", true)
        .await
        .unwrap_err();
    assert_eq!(err, "This offer is no longer available.");
    m.shutdown().await;
}

#[test]
fn reuse_offer_event_json() {
    let offer = odm_appcore::offers::OfferDto {
        offer_id: "o".into(),
        existing_id: "e".into(),
        filename: "f.zip".into(),
    };
    assert_eq!(
        serde_json::to_value(UiEvent::ReuseOffer { offer }).unwrap(),
        serde_json::json!({"type":"reuse_offer","offer":{"offer_id":"o","existing_id":"e","filename":"f.zip"}})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn suggest_lists_variants_for_master() {
    let src = tempfile::tempdir().unwrap();
    write_synthetic(src.path(), 1, 100, false);
    std::fs::write(
        src.path().join("master.m3u8"),
        "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\nlow.m3u8\n\
         #EXT-X-STREAM-INF:BANDWIDTH=2500000,RESOLUTION=1280x720\nmedia.m3u8\n",
    )
    .unwrap();
    let hls = HlsServer::start(src.path().to_path_buf(), HlsFaults::default()).await;
    let sug = api::suggest(&hls.url("/master.m3u8"), &EngineConfig::default())
        .await
        .unwrap();
    assert!(sug.is_stream);
    assert_eq!(sug.category, "video");
    assert_eq!(sug.filename, "master.mp4");
    assert_eq!(sug.variants.len(), 2);
    assert_eq!(sug.variants[0].bandwidth, 2_500_000);
    assert_eq!(sug.variants[0].url, hls.url("/media.m3u8"));
    assert_eq!(sug.variants[0].label, "1280\u{d7}720 \u{b7} 2.5 Mbit/s");
    assert_eq!(sug.variants[1].label, "640\u{d7}360 \u{b7} 800 kbit/s");

    let media = api::suggest(&hls.url("/media.m3u8"), &EngineConfig::default())
        .await
        .unwrap();
    assert!(media.is_stream);
    assert!(media.variants.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn suggest_plain_file_has_no_variants() {
    let s = TestServer::start(5000, Faults::default()).await;
    let sug = api::suggest(&s.url("/file"), &EngineConfig::default())
        .await
        .unwrap();
    assert!(!sug.is_stream);
    assert!(sug.variants.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn add_with_stream_flag_adds_hls_download_for_the_chosen_variant() {
    let src = tempfile::tempdir().unwrap();
    write_synthetic(src.path(), 1, 100, false);
    let hls = HlsServer::start(src.path().to_path_buf(), HlsFaults::default()).await;
    let d = tempfile::tempdir().unwrap();
    let m = manager(d.path()).await;
    let id = api::add(
        &m,
        AddArgs {
            url: hls.url("/master.m3u8"),
            variant_url: Some(hls.url("/media.m3u8")),
            stream: true,
            start_paused: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let (rec, _) = m.get(&id).await.unwrap().unwrap();
    assert_eq!(rec.kind, "hls");
    let meta = m.store().get_hls_meta(&id).await.unwrap().unwrap();
    assert!(meta.variant_url.ends_with("/media.m3u8"));
}

#[tokio::test(flavor = "multi_thread")]
async fn confirm_actions() {
    use odm_appcore::api::ConfirmArgs;
    use odm_appcore::confirm::Pending;
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(1 << 20, Faults::default()).await;
    let m = manager(d.path()).await;
    let pending = Pending::default();
    let add = |name: &'static str| {
        let (m, url) = (m.clone(), s.url("/file"));
        async move {
            api::add(
                &m,
                AddArgs {
                    url,
                    filename: Some(name.into()),
                    start_paused: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap()
        }
    };
    let args = |id: &str, action: &str, filename: Option<&str>| ConfirmArgs {
        id: id.into(),
        filename: filename.map(Into::into),
        save_dir: None,
        action: action.into(),
        files: None,
    };

    let a = add("a.bin").await;
    pending.push(a.clone());
    api::confirm_download(&m, &pending, args(&a, "start", Some("renamed.bin")))
        .await
        .unwrap();
    assert!(pending.list().is_empty());
    wait_completed(&m, &a).await;
    let (rec, _) = m.get(&a).await.unwrap().unwrap();
    assert_eq!(rec.filename, "renamed.bin");
    assert!(d.path().join("renamed.bin").exists());

    let b = add("b.bin").await;
    pending.push(b.clone());
    api::confirm_download(&m, &pending, args(&b, "later", Some("   ")))
        .await
        .unwrap();
    assert!(pending.list().is_empty());
    let (rec, _) = m.get(&b).await.unwrap().unwrap();
    assert_eq!(
        (rec.state, rec.filename.as_str()),
        (DownloadState::Paused, "b.bin")
    );

    let c = add("c.bin").await;
    pending.push(c.clone());
    api::confirm_download(&m, &pending, args(&c, "cancel", None))
        .await
        .unwrap();
    assert!(pending.list().is_empty());
    let deadline = Instant::now() + Duration::from_secs(10);
    while m.get(&c).await.unwrap().is_some() {
        assert!(Instant::now() < deadline, "record not removed");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let err = api::confirm_download(&m, &pending, args(&b, "nope", None))
        .await
        .unwrap_err();
    assert_eq!(err, "Unknown action.");
    // An unknown id is fine for pending, and "later" on a vanished record is not an error to surface.
    pending.push("ghost".into());
    api::confirm_download(&m, &pending, args("ghost", "later", None))
        .await
        .unwrap();
    assert!(pending.list().is_empty());
    m.shutdown().await;
}

#[test]
fn confirm_download_event_json() {
    assert_eq!(
        serde_json::to_value(UiEvent::ConfirmDownload { id: "x".into() }).unwrap(),
        serde_json::json!({"type":"confirm_download","id":"x"})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn suggest_marks_site_pages() {
    // No network: a site page is recognised from its address alone.
    let sug = api::suggest(
        "https://www.youtube.com/watch?v=jNQXAC9IVRw",
        &EngineConfig::default(),
    )
    .await
    .unwrap();
    assert!(sug.is_page);
    assert!(!sug.is_stream);
    assert_eq!(sug.filename, "video.mp4");
    assert_eq!(sug.category, "video");
    assert_eq!(sug.size, None);
    assert!(sug.resumable);
    assert!(sug.variants.is_empty());
    let s = TestServer::start(5000, Faults::default()).await;
    let plain = api::suggest(&s.url("/file"), &EngineConfig::default())
        .await
        .unwrap();
    assert!(!plain.is_page);
}

#[tokio::test(flavor = "multi_thread")]
async fn add_with_page_flag_adds_a_site_download() {
    let d = tempfile::tempdir().unwrap();
    let m = manager(d.path()).await;
    let id = api::add(
        &m,
        AddArgs {
            url: "https://www.youtube.com/watch?v=abc".into(),
            page: true,
            start_paused: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let (rec, _) = m.get(&id).await.unwrap().unwrap();
    assert_eq!(rec.kind, "site");
    assert_eq!(rec.url, "https://www.youtube.com/watch?v=abc");
    assert_eq!(rec.state, DownloadState::Paused);
}

#[tokio::test(flavor = "multi_thread")]
async fn batch_can_add_paused() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = manager(d.path()).await;
    let r = api::add_batch(&m, &s.url("/file"), None, true)
        .await
        .unwrap();
    let (rec, _) = m.get(&r.added[0]).await.unwrap().unwrap();
    assert_eq!(rec.state, DownloadState::Paused);
}

#[test]
fn start_mode_decides_whether_adds_start_paused() {
    use odm_appcore::settings::StartMode;
    assert!(!api::starts_paused(&Settings::default()));
    let paused = Settings {
        start_mode: StartMode::Paused,
        ..Settings::default()
    };
    assert!(api::starts_paused(&paused));
}

#[tokio::test(flavor = "multi_thread")]
async fn add_with_a_speed_limit_and_change_it() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(1000, Faults::default()).await;
    let m = manager(d.path()).await;
    let id = api::add(
        &m,
        AddArgs {
            url: s.url("/file"),
            start_paused: true,
            speed_limit_kbps: Some(300),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let dto = api::detail(&m, &id).await.unwrap().unwrap().download;
    assert_eq!(dto.speed_limit_kbps, Some(300));
    api::set_download_limit(&m, &id, None).await.unwrap();
    let dto = api::detail(&m, &id).await.unwrap().unwrap().download;
    assert_eq!(dto.speed_limit_kbps, None);
    api::set_download_limit(&m, &id, Some(64)).await.unwrap();
    let (rec, _) = m.get(&id).await.unwrap().unwrap();
    assert_eq!(rec.speed_limit, Some(64 * 1024));
}

#[test]
fn behaviour_event_json_shapes_are_pinned() {
    let json = serde_json::to_string(&UiEvent::ExistsPrompt {
        id: "a".into(),
        name: "f.zip".into(),
    })
    .unwrap();
    assert_eq!(json, r#"{"type":"exists_prompt","id":"a","name":"f.zip"}"#);
    let json = serde_json::to_string(&UiEvent::AfterQueuePending {
        action: odm_appcore::settings::AfterQueue::Sleep,
        seconds: 60,
    })
    .unwrap();
    assert_eq!(
        json,
        r#"{"type":"after_queue_pending","action":"sleep","seconds":60}"#
    );
    let json = serde_json::to_string(&UiEvent::AfterQueueCancelled).unwrap();
    assert_eq!(json, r#"{"type":"after_queue_cancelled"}"#);
    let json = serde_json::to_string(&UiEvent::AfterQueueDone).unwrap();
    assert_eq!(json, r#"{"type":"after_queue_done"}"#);
    let json = serde_json::to_string(&UiEvent::ShutdownFailed {
        message: "ODM couldn't shut down the PC.".into(),
    })
    .unwrap();
    assert_eq!(
        json,
        r#"{"type":"shutdown_failed","message":"ODM couldn't shut down the PC."}"#
    );
}

#[test]
fn rows_say_why_a_download_is_paused() {
    let mut r = odm_store::DownloadRecord::new_file("a", "https://e.com/f", "f", "C:/dl");
    r.state = DownloadState::Paused;
    r.pause_reason = Some("schedule".into());
    let dto = odm_appcore::dto::to_dto(&r, 0);
    assert_eq!(dto.pause_reason.as_deref(), Some("schedule"));
    let json = serde_json::to_value(&dto).unwrap();
    assert_eq!(json["pause_reason"], "schedule");
    assert!(json["speed_limit_kbps"].is_null());
}

/// Review Focus 4: an invalid proxy address coming through the settings command is refused
/// with the page's message and never written; the stored settings keep the old value.
#[tokio::test(flavor = "multi_thread")]
async fn proxy_url_validation() {
    use odm_appcore::settings::{ProxyMode, PROXY_BAD_PORT, PROXY_FORMAT, PROXY_NO_PORT};
    let data = tempfile::tempdir().unwrap();
    let m = manager(data.path()).await;
    let path = data.path().join("settings.json");
    let good = Settings {
        download_dir: data.path().to_string_lossy().into_owned(),
        proxy_mode: ProxyMode::Manual,
        proxy_url: "http://127.0.0.1:8080".into(),
        ..Default::default()
    };
    api::save_settings(&m, &path, good.clone()).await.unwrap();
    for (url, msg) in [
        ("127.0.0.1:8080", PROXY_FORMAT),
        ("proxy.local", PROXY_FORMAT),
        ("http://proxy.local", PROXY_NO_PORT),
        ("http://proxy.local:99999", PROXY_BAD_PORT),
        ("http://proxy.local:0", PROXY_BAD_PORT),
    ] {
        let bad = Settings {
            proxy_url: url.into(),
            ..good.clone()
        };
        let err = api::save_settings(&m, &path, bad).await.unwrap_err();
        assert_eq!(err, msg, "{url}");
        assert_eq!(Settings::load(&path).0.proxy_url, "http://127.0.0.1:8080");
    }
    // The same rule holds when the proxy is not in use: a bad address is never stored.
    let bad = Settings {
        proxy_mode: ProxyMode::System,
        proxy_url: "nope".into(),
        ..good.clone()
    };
    assert!(api::save_settings(&m, &path, bad).await.is_err());
    assert_eq!(Settings::load(&path).0, good);
}

#[derive(Default)]
struct FakeAutostart {
    calls: std::sync::Mutex<Vec<bool>>,
    fail: bool,
}

impl odm_appcore::autostart::Autostart for FakeAutostart {
    fn set_enabled(&self, on: bool) -> Result<(), String> {
        self.calls.lock().unwrap().push(on);
        if self.fail {
            Err("registry says no".into())
        } else {
            Ok(())
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn start_with_windows_round_trips_and_calls_autostart_on_change() {
    let data = tempfile::tempdir().unwrap();
    let m = manager(data.path()).await;
    let path = data.path().join("settings.json");
    let auto = FakeAutostart::default();
    let before = Settings {
        download_dir: data.path().to_string_lossy().into_owned(),
        ..Default::default()
    };
    // Unchanged: the registry is not touched.
    let (saved, _) = api::update_settings(&m, &path, &before, before.clone(), &auto)
        .await
        .unwrap();
    assert!(auto.calls.lock().unwrap().is_empty());
    let on = Settings {
        start_with_windows: true,
        ..saved
    };
    let (saved, _) = api::update_settings(&m, &path, &before, on, &auto)
        .await
        .unwrap();
    assert!(saved.start_with_windows);
    assert!(Settings::load(&path).0.start_with_windows);
    assert_eq!(*auto.calls.lock().unwrap(), [true]);
    let off = Settings {
        start_with_windows: false,
        ..saved.clone()
    };
    api::update_settings(&m, &path, &saved, off, &auto)
        .await
        .unwrap();
    assert!(!Settings::load(&path).0.start_with_windows);
    assert_eq!(*auto.calls.lock().unwrap(), [true, false]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_autostart_change_saves_nothing() {
    let data = tempfile::tempdir().unwrap();
    let m = manager(data.path()).await;
    let path = data.path().join("settings.json");
    let auto = FakeAutostart {
        fail: true,
        ..Default::default()
    };
    let before = Settings {
        download_dir: data.path().to_string_lossy().into_owned(),
        ..Default::default()
    };
    api::save_settings(&m, &path, before.clone()).await.unwrap();
    let on = Settings {
        start_with_windows: true,
        ..before.clone()
    };
    let err = api::update_settings(&m, &path, &before, on, &auto)
        .await
        .unwrap_err();
    assert!(err.contains("starts with Windows"), "{err}");
    assert!(!Settings::load(&path).0.start_with_windows);
    // An invalid proxy is refused before the registry is touched.
    let ok = FakeAutostart::default();
    let bad = Settings {
        start_with_windows: true,
        proxy_url: "nope".into(),
        ..before.clone()
    };
    assert!(api::update_settings(&m, &path, &before, bad, &ok)
        .await
        .is_err());
    assert!(ok.calls.lock().unwrap().is_empty());
}

/// "Start new downloads: Paused" adds paused, unless the dialog's "Start now" was pressed.
#[test]
fn start_now_overrides_the_paused_start_mode() {
    use odm_appcore::settings::StartMode;
    let paused = Settings {
        start_mode: StartMode::Paused,
        ..Settings::default()
    };
    let plain = api::with_start_mode(&paused, AddArgs::default());
    assert!(plain.start_paused && !plain.start_now);
    let now = api::with_start_mode(
        &paused,
        AddArgs {
            start_now: true,
            ..AddArgs::default()
        },
    );
    assert!(!now.start_paused && now.start_now);
    let auto = api::with_start_mode(&Settings::default(), AddArgs::default());
    assert!(!auto.start_paused);
}

/// "Start now" runs at once while the queue schedule is closed; a plain add waits.
#[tokio::test(flavor = "multi_thread")]
async fn start_now_bypasses_a_closed_queue_window() {
    use odm_engine::schedule::{LocalNow, Window};
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(64 << 10, Faults::default()).await;
    // Always Monday 03:00, outside the 13:00–14:00 queue window.
    struct Closed;
    impl odm_engine::schedule::Clock for Closed {
        fn now(&self) -> LocalNow {
            LocalNow {
                unix_ms: odm_store::now_ms(),
                weekday: 0,
                minute: 3 * 60,
            }
        }
    }
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
        std::sync::Arc::new(Closed),
    )
    .await
    .unwrap();
    let waits = api::add(
        &m,
        AddArgs {
            url: s.url("/file"),
            filename: Some("waits.bin".into()),
            ..AddArgs::default()
        },
    )
    .await
    .unwrap();
    let now = api::add(
        &m,
        AddArgs {
            url: s.url("/file"),
            filename: Some("now.bin".into()),
            start_now: true,
            ..AddArgs::default()
        },
    )
    .await
    .unwrap();
    wait_completed(&m, &now).await;
    let (r, _) = m.get(&waits).await.unwrap().unwrap();
    assert_eq!(r.state, DownloadState::Queued);
}
