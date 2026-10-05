mod common;

use common::{fixture, req};
use odm_appcore::ipc::{dispatch, now_ms};
use odm_core::ipc::{cmd, code};
use odm_core::DownloadState;
use odm_engine::AddRequest;
use odm_testserver::hls::{write_synthetic, HlsFaults, HlsServer};
use odm_testserver::{Faults, TestServer};
use serde_json::json;

async fn server() -> TestServer {
    TestServer::start(8 << 20, Faults::default()).await
}

#[tokio::test(flavor = "multi_thread")]
async fn ping_returns_version() {
    let f = fixture().await;
    let r = dispatch(&f.ctx, req(cmd::PING, json!({}))).await;
    assert!(r.ok);
    assert_eq!(r.id.as_deref(), Some("r1"));
    assert_eq!(r.payload.unwrap()["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test(flavor = "multi_thread")]
async fn add_download_capture_adds_with_cookies_and_referer() {
    let f = fixture().await;
    let s = server().await;
    let r = dispatch(
        &f.ctx,
        req(
            cmd::ADD_DOWNLOAD,
            json!({
                "url": s.url("/file"),
                "filename": "C:\\Users\\x\\Downloads\\movie.mp4",
                "referrer": "https://site/p",
                "cookies": "sid=abc",
                "filesize": 8388608,
                "expires_at_ms": now_ms() + 60000,
            }),
        ),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let id = r.payload.unwrap()["download_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (rec, _) = f.ctx.manager.get(&id).await.unwrap().unwrap();
    assert_eq!(rec.filename, "movie.mp4");
    assert_eq!(rec.page_url.as_deref(), Some("https://site/p"));
    let headers = f.ctx.manager.store().get_headers(&id).await.unwrap();
    assert!(headers.contains(&("Cookie".into(), "sid=abc".into())));
    assert!(headers.contains(&("Referer".into(), "https://site/p".into())));
}

#[tokio::test(flavor = "multi_thread")]
async fn expired_capture_is_refused_and_not_added() {
    let f = fixture().await;
    let s = server().await;
    let r = dispatch(
        &f.ctx,
        req(
            cmd::ADD_DOWNLOAD,
            json!({"url": s.url("/file"), "filesize": 8388608, "expires_at_ms": 1}),
        ),
    )
    .await;
    assert!(!r.ok);
    assert_eq!(r.error.unwrap().code, code::EXPIRED);
    assert!(f.ctx.manager.list().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn capture_filters_apply_but_menu_bypasses_them() {
    let f = fixture().await;
    let s = server().await;
    f.ctx.settings.lock().unwrap().capture_enabled = false;
    let payload = json!({"url": s.url("/file"), "filesize": 8388608});
    let r = dispatch(&f.ctx, req(cmd::ADD_DOWNLOAD, payload.clone())).await;
    let e = r.error.unwrap();
    assert_eq!(
        (e.code.as_str(), e.message.as_str()),
        (code::SKIPPED, "capture is off")
    );
    let mut menu = payload;
    menu["source"] = json!("menu");
    assert!(dispatch(&f.ctx, req(cmd::ADD_DOWNLOAD, menu)).await.ok);

    f.ctx.settings.lock().unwrap().capture_enabled = true;
    let small = json!({"url": s.url("/file2"), "filesize": 10});
    let r = dispatch(&f.ctx, req(cmd::ADD_DOWNLOAD, small)).await;
    assert_eq!(r.error.unwrap().code, code::SKIPPED);
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_payload_is_a_typed_error() {
    let f = fixture().await;
    let r = dispatch(&f.ctx, req(cmd::ADD_DOWNLOAD, json!({"url": 5}))).await;
    assert_eq!(r.error.unwrap().code, code::BAD_PAYLOAD);
    let r = dispatch(&f.ctx, req(cmd::ADD_DOWNLOAD, serde_json::Value::Null)).await;
    assert_eq!(r.error.unwrap().code, code::BAD_PAYLOAD);
    let r = dispatch(&f.ctx, req("FOO", json!({}))).await;
    let e = r.error.unwrap();
    assert_eq!(e.code, code::UNKNOWN_COMMAND);
    assert_eq!(e.message, "Unknown command: FOO");
    assert_eq!(r.id.as_deref(), Some("r1"));
}

#[tokio::test(flavor = "multi_thread")]
async fn hostile_filename_is_sanitized() {
    let f = fixture().await;
    let s = server().await;
    let r = dispatch(
        &f.ctx,
        req(
            cmd::ADD_DOWNLOAD,
            json!({"url": s.url("/file"), "filename": "..\\..\\evil\u{202E}txt.exe", "source": "menu"}),
        ),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let id = r.payload.unwrap()["download_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (rec, _) = f.ctx.manager.get(&id).await.unwrap().unwrap();
    for bad in ["\\", "/", "..", "\u{202E}"] {
        assert!(!rec.filename.contains(bad), "{:?}", rec.filename);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn summary_counts_and_speed() {
    let f = fixture().await;
    let s = server().await;
    for n in ["a.bin", "b.bin"] {
        f.ctx
            .manager
            .add(AddRequest {
                filename: Some(n.into()),
                start_paused: true,
                ..AddRequest::new(s.url("/file"))
            })
            .await
            .unwrap();
    }
    f.ctx
        .speeds
        .lock()
        .unwrap()
        .apply(&odm_engine::EngineEvent::Progress(
            odm_engine::ProgressSnapshot {
                id: "x".into(),
                bytes: 0,
                total: None,
                speed_bps: 1000,
                eta_s: None,
                chunks: vec![],
                segments: None,
            },
        ));
    let r = dispatch(&f.ctx, req(cmd::GET_SUMMARY, json!({}))).await;
    assert_eq!(
        r.payload.unwrap(),
        json!({"active": 0, "queued": 0, "total_speed_bps": 1000})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_all_resume_all_show_window() {
    let f = fixture().await;
    for c in [cmd::PAUSE_ALL, cmd::RESUME_ALL, cmd::SHOW_WINDOW] {
        let r = dispatch(&f.ctx, req(c, json!({}))).await;
        assert!(r.ok, "{c}: {:?}", r.error);
    }
    assert_eq!(f.shows(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn add_stream_adds_hls_download() {
    let f = fixture().await;
    let src = tempfile::tempdir().unwrap();
    write_synthetic(src.path(), 2, 1000, false);
    let hls = HlsServer::start(src.path().to_path_buf(), HlsFaults::default()).await;
    let r = dispatch(
        &f.ctx,
        req(
            cmd::ADD_STREAM,
            json!({
                "page_url": "https://site/watch",
                "page_title": "Clip",
                "playlist_url": hls.url("/media.m3u8"),
                "cookies": "a=1",
            }),
        ),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let id = r.payload.unwrap()["download_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (rec, _) = f.ctx.manager.get(&id).await.unwrap().unwrap();
    assert_eq!(rec.kind, "hls");
    assert_eq!(rec.page_title.as_deref(), Some("Clip"));
    assert_eq!(rec.filename, "Clip.mp4");
    let headers = f.ctx.manager.store().get_headers(&id).await.unwrap();
    assert!(headers.contains(&("Cookie".into(), "a=1".into())));
    assert!(headers.contains(&("Referer".into(), "https://site/watch".into())));
    let _ = f.ctx.manager.cancel(&id, true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn add_stream_rejects_bad_payload_and_scheme() {
    let f = fixture().await;
    let r = dispatch(&f.ctx, req(cmd::ADD_STREAM, json!({}))).await;
    assert_eq!(r.error.unwrap().code, code::BAD_PAYLOAD);
    let bad = json!({"page_url": "https://p", "playlist_url": "ftp://x/a.m3u8"});
    let r = dispatch(&f.ctx, req(cmd::ADD_STREAM, bad)).await;
    assert_eq!(r.error.unwrap().code, code::ADD_FAILED);
}

#[tokio::test(flavor = "multi_thread")]
async fn matching_paused_download_creates_an_offer() {
    let f = fixture().await;
    let s = server().await;
    let existing = f
        .ctx
        .manager
        .add(AddRequest {
            filename: Some("movie.mp4".into()),
            start_paused: true,
            ..AddRequest::new(s.url("/file"))
        })
        .await
        .unwrap();
    let (rec, _) = f.ctx.manager.get(&existing).await.unwrap().unwrap();
    assert_eq!(rec.state, DownloadState::Paused);
    let r = dispatch(
        &f.ctx,
        req(
            cmd::ADD_DOWNLOAD,
            json!({"url": s.url("/file"), "filename": "movie.mp4", "source": "menu"}),
        ),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    assert_eq!(r.payload.unwrap()["download_id"], existing.as_str());
    assert_eq!(f.ctx.offers.list().len(), 1);
    assert_eq!((f.offers_seen(), f.shows()), (1, 1));
    assert_eq!(f.ctx.manager.list().await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn confirm_setting_adds_paused_and_asks() {
    let f = fixture().await;
    let s = server().await;
    f.ctx.settings.lock().unwrap().confirm_browser_downloads = true;
    let r = dispatch(
        &f.ctx,
        req(
            cmd::ADD_DOWNLOAD,
            json!({"url": s.url("/file"), "filename": "a.bin", "source": "menu"}),
        ),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let id = r.payload.unwrap()["download_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (rec, _) = f.ctx.manager.get(&id).await.unwrap().unwrap();
    assert_eq!(rec.state, DownloadState::Paused);
    assert_eq!(f.ctx.pending.list(), vec![id.clone()]);
    assert_eq!((f.confirms().len(), f.shows()), (1, 1));
    assert_eq!(f.confirms().last().cloned(), Some(id));

    f.ctx.settings.lock().unwrap().confirm_browser_downloads = false;
    let r = dispatch(
        &f.ctx,
        req(
            cmd::ADD_DOWNLOAD,
            json!({"url": s.url("/file2"), "filename": "b.bin", "source": "menu"}),
        ),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let id2 = r.payload.unwrap()["download_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (rec2, _) = f.ctx.manager.get(&id2).await.unwrap().unwrap();
    assert_ne!(rec2.state, DownloadState::Paused);
    assert_eq!(f.ctx.pending.list().len(), 1);
    assert_eq!((f.confirms().len(), f.shows()), (1, 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn confirm_setting_applies_to_streams_too() {
    let f = fixture().await;
    f.ctx.settings.lock().unwrap().confirm_browser_downloads = true;
    let r = dispatch(
        &f.ctx,
        req(
            cmd::ADD_STREAM,
            json!({"page_url": "https://p", "playlist_url": "http://127.0.0.1:9/a.m3u8"}),
        ),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let id = r.payload.unwrap()["download_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (rec, _) = f.ctx.manager.get(&id).await.unwrap().unwrap();
    assert_eq!(rec.state, DownloadState::Paused);
    assert_eq!(f.ctx.pending.list(), vec![id]);
    assert_eq!((f.confirms().len(), f.shows()), (1, 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn add_page_adds_site_download() {
    let f = fixture().await;
    f.ctx.settings.lock().unwrap().ytdlp_path = "Z:/nope/yt-dlp.exe".into();
    let r = dispatch(
        &f.ctx,
        req(
            cmd::ADD_PAGE,
            json!({
                "page_url": "https://www.youtube.com/watch?v=abc",
                "page_title": "Clip",
                "cookies": "a=1",
            }),
        ),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let id = r.payload.unwrap()["download_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (rec, _) = f.ctx.manager.get(&id).await.unwrap().unwrap();
    assert_eq!(rec.kind, "site");
    assert_eq!(rec.url, "https://www.youtube.com/watch?v=abc");
    assert_eq!(rec.filename, "Clip.mp4");
    let headers = f.ctx.manager.store().get_headers(&id).await.unwrap();
    assert!(headers.contains(&("Cookie".into(), "a=1".into())));
    assert!(headers.contains(&(
        "Referer".into(),
        "https://www.youtube.com/watch?v=abc".into()
    )));
    let _ = f.ctx.manager.cancel(&id, true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn add_page_respects_confirm_setting() {
    let f = fixture().await;
    f.ctx.settings.lock().unwrap().confirm_browser_downloads = true;
    let r = dispatch(
        &f.ctx,
        req(
            cmd::ADD_PAGE,
            json!({"page_url": "https://www.youtube.com/watch?v=abc"}),
        ),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let id = r.payload.unwrap()["download_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (rec, _) = f.ctx.manager.get(&id).await.unwrap().unwrap();
    assert_eq!(rec.state, DownloadState::Paused);
    assert_eq!(f.ctx.pending.list(), vec![id]);
    assert_eq!((f.confirms().len(), f.shows()), (1, 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn add_page_rejects_bad_payload_and_scheme() {
    let f = fixture().await;
    let r = dispatch(&f.ctx, req(cmd::ADD_PAGE, json!({}))).await;
    assert_eq!(r.error.unwrap().code, code::BAD_PAYLOAD);
    let r = dispatch(&f.ctx, req(cmd::ADD_PAGE, json!({"page_url": "ftp://x/y"}))).await;
    assert_eq!(r.error.unwrap().code, code::ADD_FAILED);
}
