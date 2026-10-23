//! Torrent intake in the app core: sources, IPC, confirm with files, settings, detail.
#[allow(dead_code)]
mod common;

use axum::{routing::get, Router};
use bytes::Bytes;
use common::{fixture, req};
use odm_appcore::api::{self, ConfirmArgs};
use odm_appcore::torrent::{resolve, TorrentInput, MAX_TORRENT_BYTES};
use odm_appcore::Settings;
use odm_core::ipc::cmd;
use odm_core::DownloadState;
use odm_torrent::TorrentSource;
use serde_json::json;
use std::time::{Duration, Instant};

const HEX: &str = "abcdef0123456789abcdef0123456789abcdef01";

/// A valid two-file .torrent (one 16 KiB piece).
fn torrent_bytes() -> Vec<u8> {
    let mut t = b"d4:infod5:filesld6:lengthi3e4:pathl5:x.txteed6:lengthi5e4:pathl5:y.bineee4:name3:Bar12:piece lengthi16384e6:pieces20:".to_vec();
    t.extend_from_slice(&[7u8; 20]);
    t.extend_from_slice(b"ee");
    t
}

/// Serves `/ok.torrent`, `/big.torrent` (11 MB), `/slow.torrent` (2 s delay) and
/// `/needs-cookie.torrent` (403 without `Cookie: k=v`).
async fn web() -> String {
    let app = Router::new()
        .route("/ok.torrent", get(|| async { torrent_bytes() }))
        .route("/big.torrent", get(|| async { vec![b'x'; 11 * 1024 * 1024] }))
        .route(
            "/slow.torrent",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(2)).await;
                torrent_bytes()
            }),
        )
        .route(
            "/needs-cookie.torrent",
            get(|h: axum::http::HeaderMap| async move {
                if h.get("cookie").and_then(|v| v.to_str().ok()) == Some("k=v") {
                    (axum::http::StatusCode::OK, torrent_bytes())
                } else {
                    (axum::http::StatusCode::FORBIDDEN, Vec::new())
                }
            }),
        );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    base
}

fn http() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn resolves_magnet_text_and_file() {
    let m = resolve(TorrentInput::Text(format!("  magnet:?xt=urn:btih:{HEX}  ")), &http(), Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(m, TorrentSource::Magnet(format!("magnet:?xt=urn:btih:{HEX}")));
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.torrent");
    std::fs::write(&p, torrent_bytes()).unwrap();
    let f = resolve(TorrentInput::File(p), &http(), Duration::from_secs(1)).await.unwrap();
    assert_eq!(f, TorrentSource::Bytes(Bytes::from(torrent_bytes())));
    let bad = resolve(TorrentInput::Text("hello".into()), &http(), Duration::from_secs(1)).await;
    assert_eq!(bad, Err("This isn't a valid magnet link or torrent file.".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn url_fetch_sends_headers() {
    let base = web().await;
    let src = resolve(
        TorrentInput::Url {
            url: format!("{base}/needs-cookie.torrent"),
            headers: vec![("Cookie".into(), "k=v".into())],
        },
        &http(),
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert_eq!(src, TorrentSource::Bytes(Bytes::from(torrent_bytes())));
    let denied = resolve(TorrentInput::Url { url: format!("{base}/needs-cookie.torrent"), headers: vec![] }, &http(), Duration::from_secs(5)).await;
    assert_eq!(denied, Err("The torrent file couldn't be downloaded (HTTP 403).".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn url_fetch_too_large() {
    let base = web().await;
    assert_eq!(MAX_TORRENT_BYTES, 10 * 1024 * 1024);
    let r = resolve(TorrentInput::Url { url: format!("{base}/big.torrent"), headers: vec![] }, &http(), Duration::from_secs(10)).await;
    assert_eq!(r, Err("The torrent file is larger than 10 MB.".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn url_fetch_timeout() {
    let base = web().await;
    let start = Instant::now();
    let r = resolve(TorrentInput::Url { url: format!("{base}/slow.torrent"), headers: vec![] }, &http(), Duration::from_millis(200)).await;
    assert_eq!(r, Err("The torrent file didn't download in time.".to_string()));
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[tokio::test(flavor = "multi_thread")]
async fn ipc_add_torrent_magnet() {
    let f = fixture().await;
    // Torrents always ask, even with the browser confirm setting off (the fixture's default).
    let r = odm_appcore::ipc::dispatch(
        &f.ctx,
        req(cmd::ADD_TORRENT, json!({"link": format!("magnet:?xt=urn:btih:{HEX}&dn=Foo"), "kind": "magnet", "source": "capture"})),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let id = r.payload.unwrap()["download_id"].as_str().unwrap().to_string();
    let (rec, _) = f.ctx.manager.get(&id).await.unwrap().unwrap();
    assert_eq!((rec.kind.as_str(), rec.state), ("torrent", DownloadState::Paused));
    assert_eq!(f.ctx.pending.list(), vec![id.clone()]);
    assert_eq!(f.confirms(), vec![id.clone()]);
    assert_eq!(f.shows(), 1);

    // The same torrent again: no second row, no second dialog, the window comes up.
    let r = odm_appcore::ipc::dispatch(
        &f.ctx,
        req(cmd::ADD_TORRENT, json!({"link": format!("magnet:?xt=urn:btih:{HEX}"), "kind": "magnet"})),
    )
    .await;
    assert!(r.ok);
    assert_eq!(r.payload.unwrap()["download_id"].as_str().unwrap(), id);
    assert_eq!(f.confirms().len(), 1);
    assert_eq!(f.shows(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn ipc_add_torrent_url_fetches() {
    let f = fixture().await;
    let base = web().await;
    let r = odm_appcore::ipc::dispatch(
        &f.ctx,
        req(
            cmd::ADD_TORRENT,
            json!({"link": format!("{base}/needs-cookie.torrent"), "kind": "url", "cookies": "k=v", "referrer": "https://site.example/page"}),
        ),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let id = r.payload.unwrap()["download_id"].as_str().unwrap().to_string();
    assert_eq!(f.ctx.manager.torrent_files(&id).await.unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn ipc_add_torrent_bad_link() {
    let f = fixture().await;
    let r = odm_appcore::ipc::dispatch(&f.ctx, req(cmd::ADD_TORRENT, json!({"link": "nope", "kind": "magnet"}))).await;
    assert!(!r.ok);
    assert_eq!(r.error.unwrap().message, "This isn't a valid magnet link or torrent file.");
}

#[tokio::test(flavor = "multi_thread")]
async fn confirm_torrent_with_files() {
    let f = fixture().await;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.torrent");
    std::fs::write(&p, torrent_bytes()).unwrap();
    let m = &f.ctx.manager;
    let added = api::add_torrent(m, &f.ctx.pending, &http(), TorrentInput::File(p)).await.unwrap();
    assert_eq!(f.ctx.pending.list(), vec![added.id.clone()]);
    api::confirm_download(
        m,
        &f.ctx.pending,
        ConfirmArgs { id: added.id.clone(), filename: None, save_dir: None, action: "later".into(), files: Some(vec![1]) },
    )
    .await
    .unwrap();
    let sel: Vec<bool> = m.torrent_files(&added.id).await.unwrap().iter().map(|f| f.selected).collect();
    assert_eq!(sel, vec![false, true]);
    assert!(f.ctx.pending.list().is_empty());
    let (rec, _) = m.get(&added.id).await.unwrap().unwrap();
    assert_eq!(rec.size, Some(5));
}

#[tokio::test(flavor = "multi_thread")]
async fn confirm_download_all_without_meta() {
    let f = fixture().await;
    let m = &f.ctx.manager;
    let added = api::add_torrent(m, &f.ctx.pending, &http(), TorrentInput::Text(format!("magnet:?xt=urn:btih:{HEX}")))
        .await
        .unwrap();
    api::confirm_download(
        m,
        &f.ctx.pending,
        ConfirmArgs { id: added.id.clone(), filename: None, save_dir: None, action: "start".into(), files: None },
    )
    .await
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while m.get(&added.id).await.unwrap().unwrap().0.state == DownloadState::Paused {
        assert!(Instant::now() < deadline, "still paused after Download all");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[test]
fn settings_torrent_defaults_and_parse() {
    let d = Settings::default();
    assert_eq!((d.torrent_port, d.torrent_upnp, d.torrent_upload_kbps, d.torrent_max_peers), (0, true, Some(100), 100));
    let mut s = d.clone();
    assert!(s.ensure_torrent_port());
    assert!((49152..=65535).contains(&s.torrent_port));
    let kept = s.torrent_port;
    assert!(!s.ensure_torrent_port());
    assert_eq!(s.torrent_port, kept);

    let (p, w) = Settings::from_json(r#"{"torrent_port": 51000, "torrent_upload_kbps": null, "torrent_max_peers": 5}"#);
    assert_eq!(p.torrent_port, 51000);
    assert_eq!(p.torrent_upload_kbps, None);
    assert_eq!(p.torrent_max_peers, 10, "clamped: {w:?}");
    let cfg = p.to_engine_config_with(std::path::PathBuf::from("tools"));
    assert_eq!(cfg.torrent.listen_port, 51000);
    assert_eq!(cfg.torrent.upload_bps, None);
    assert_eq!(cfg.torrent.peer_limit, 10);
    let (z, _) = Settings::from_json(r#"{"torrent_upload_kbps": 0}"#);
    assert_eq!(z.to_engine_config_with("t".into()).torrent.upload_bps, Some(0));
}

#[test]
fn socks_only_proxy_reaches_torrent() {
    use odm_appcore::settings::ProxyMode;
    let mut s = Settings { proxy_mode: ProxyMode::Manual, proxy_url: "socks5://127.0.0.1:1080".into(), ..Settings::default() };
    assert_eq!(s.to_engine_config_with("t".into()).torrent.socks_proxy.as_deref(), Some("socks5://127.0.0.1:1080"));
    s.proxy_url = "http://127.0.0.1:8080".into();
    assert_eq!(s.to_engine_config_with("t".into()).torrent.socks_proxy, None);
    s.proxy_mode = ProxyMode::System;
    s.proxy_url = "socks5://127.0.0.1:1080".into();
    assert_eq!(s.to_engine_config_with("t".into()).torrent.socks_proxy, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn detail_has_torrent_section() {
    let f = fixture().await;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.torrent");
    std::fs::write(&p, torrent_bytes()).unwrap();
    let m = &f.ctx.manager;
    let added = api::add_torrent(m, &f.ctx.pending, &http(), TorrentInput::File(p)).await.unwrap();
    let d = api::detail(m, &added.id).await.unwrap().unwrap();
    assert_eq!(d.download.kind, "torrent");
    assert_eq!(d.download.has_meta, Some(true));
    assert_eq!(d.download.info_hash.as_deref().map(str::len), Some(40));
    let t = d.torrent.expect("torrent section");
    assert_eq!(t.files.iter().map(|f| (f.path.as_str(), f.size, f.selected)).collect::<Vec<_>>(), vec![("x.txt", 3, true), ("y.bin", 5, true)]);
    assert!(t.pieces.is_none(), "not in the session yet");
    // Plain downloads have no torrent section.
    let list = api::list(m).await.unwrap();
    assert!(list.iter().all(|d| d.kind == "torrent"));
}

#[tokio::test(flavor = "multi_thread")]
async fn saving_port_zero_picks_one() {
    let f = fixture().await;
    let dir = tempfile::tempdir().unwrap();
    let (saved, _) = api::save_settings(&f.ctx.manager, &dir.path().join("s.json"), Settings::default())
        .await
        .unwrap();
    assert!((49152..=65535).contains(&saved.torrent_port), "{}", saved.torrent_port);
}
