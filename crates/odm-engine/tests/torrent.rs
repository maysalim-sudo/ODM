//! Torrent downloads in the manager, driven by the fake torrent client.
use bytes::Bytes;
use odm_core::DownloadState;
use odm_engine::{EngineConfig, EngineEvent, ManagerHandle, TorrentRequest};
use odm_store::{DownloadRecord, Store};
use odm_torrent::fake::{FakeCall, FakeScript, FakeTorrentClient};
use odm_torrent::{InfoHash, TorrentMeta, TorrentFile, TorrentSettings, TorrentSource};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

const HEX: &str = "abcdef0123456789abcdef0123456789abcdef01";

fn hash() -> InfoHash {
    InfoHash::parse(HEX).unwrap()
}

fn magnet(extra: &str) -> TorrentSource {
    TorrentSource::Magnet(format!("magnet:?xt=urn:btih:{HEX}&dn=Foo{extra}"))
}

fn meta() -> TorrentMeta {
    TorrentMeta {
        name: "Foo".into(),
        files: vec![
            TorrentFile { idx: 0, path: "a.bin".into(), size: 100 },
            TorrentFile { idx: 1, path: "sub/b.bin".into(), size: 50 },
        ],
        torrent_bytes: Bytes::from_static(b"d4:infod4:name3:Fooee"),
    }
}

/// A valid two-file .torrent (one 16 KiB piece) and its info hash.
fn torrent_bytes() -> Bytes {
    let mut t = b"d4:infod5:filesld6:lengthi3e4:pathl5:x.txteed6:lengthi5e4:pathl5:y.bineee4:name3:Bar12:piece lengthi16384e6:pieces20:".to_vec();
    t.extend_from_slice(&[7u8; 20]);
    t.extend_from_slice(b"ee");
    Bytes::from(t)
}

fn cfg(dir: &Path) -> EngineConfig {
    EngineConfig {
        download_dir: dir.join("dl"),
        progress_interval: Duration::from_millis(50),
        persist_interval: Duration::from_millis(100),
        proxy: odm_engine::config::ProxyConfig::Direct,
        torrent: TorrentSettings {
            state_dir: dir.join("state"),
            ..TorrentSettings::default()
        },
        ..EngineConfig::default()
    }
}

async fn start(dir: &Path) -> (ManagerHandle, Store, Arc<FakeTorrentClient>) {
    let store = Store::open_in_memory().unwrap();
    let fake = FakeTorrentClient::new();
    let m = ManagerHandle::start_with_torrents(cfg(dir), store.clone(), fake.clone())
        .await
        .unwrap();
    (m, store, fake)
}

async fn row(store: &Store, id: &str) -> DownloadRecord {
    store.get_download(id).await.unwrap().expect("record")
}

async fn wait_for<F: Fn(&EngineEvent) -> bool>(rx: &mut tokio::sync::broadcast::Receiver<EngineEvent>, f: F) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, rx.recv()).await {
            Ok(Ok(e)) if f(&e) => return,
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            other => panic!("event not seen: {other:?}"),
        }
    }
}

#[tokio::test]
async fn magnet_adds_paused_row_then_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake) = start(dir.path()).await;
    fake.script(&hash(), FakeScript { meta: Some(meta()), metadata_after: Some(Duration::from_millis(20)), ..Default::default() });
    let mut rx = m.subscribe();
    let added = m.add_torrent(TorrentRequest { source: magnet(""), save_dir: None }).await.unwrap();
    assert!(!added.existing);
    let r = row(&store, &added.id).await;
    assert_eq!((r.kind.as_str(), r.state), ("torrent", DownloadState::Paused));
    assert_eq!(r.info_hash.as_deref(), Some(HEX));
    assert_eq!(r.filename, "Foo");
    assert!(store.get_torrent_files(&added.id).await.unwrap().is_empty());

    let id = added.id.clone();
    wait_for(&mut rx, |e| matches!(e, EngineEvent::TorrentMetadata { id: i } if *i == id)).await;
    let files = m.torrent_files(&added.id).await.unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|f| f.selected));
    assert_eq!(row(&store, &added.id).await.size, Some(150));
    let tm = store.get_torrent_meta(&added.id).await.unwrap().unwrap();
    assert!(tm.has_meta);
    // The metadata is kept as a .torrent file so the download need not fetch it again.
    assert!(tm.source.ends_with(".torrent"), "{}", tm.source);
    assert!(Path::new(&tm.source).exists());
}

#[tokio::test]
async fn torrent_bytes_have_files_immediately() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake) = start(dir.path()).await;
    let added = m
        .add_torrent(TorrentRequest { source: TorrentSource::Bytes(torrent_bytes()), save_dir: Some(dir.path().join("here")) })
        .await
        .unwrap();
    let r = row(&store, &added.id).await;
    assert_eq!(r.filename, "Bar");
    assert_eq!(r.size, Some(8));
    assert_eq!(Path::new(&r.save_dir), dir.path().join("here"));
    assert_eq!(m.torrent_files(&added.id).await.unwrap().len(), 2);
    let tm = store.get_torrent_meta(&added.id).await.unwrap().unwrap();
    assert!(Path::new(&tm.source).exists());
    assert!(!fake.calls().iter().any(|c| matches!(c, FakeCall::Fetch(_))));
}

#[tokio::test]
async fn default_folder_is_the_torrent_category_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake) = start(dir.path()).await;
    fake.script(&hash(), FakeScript { metadata_after: None, ..Default::default() });
    let added = m.add_torrent(TorrentRequest { source: magnet(""), save_dir: None }).await.unwrap();
    assert_eq!(Path::new(&row(&store, &added.id).await.save_dir), dir.path().join("dl").join("Torrents"));
}

#[tokio::test]
async fn duplicate_merges_trackers() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake) = start(dir.path()).await;
    fake.script(&hash(), FakeScript { metadata_after: None, ..Default::default() });
    let a = m.add_torrent(TorrentRequest { source: magnet("&tr=udp%3A%2F%2Fa%3A1"), save_dir: None }).await.unwrap();
    let b = m.add_torrent(TorrentRequest { source: magnet("&tr=udp%3A%2F%2Fb%3A2"), save_dir: None }).await.unwrap();
    assert!(b.existing);
    assert_eq!(a.id, b.id);
    assert_eq!(store.list_downloads().await.unwrap().len(), 1);
    let tm = store.get_torrent_meta(&a.id).await.unwrap().unwrap();
    assert_eq!(tm.trackers, vec!["udp://a:1".to_string(), "udp://b:2".to_string()]);
}

#[tokio::test]
async fn invalid_source_creates_no_row() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, _fake) = start(dir.path()).await;
    let err = m
        .add_torrent(TorrentRequest { source: TorrentSource::Magnet("http://x/y".into()), save_dir: None })
        .await
        .unwrap_err();
    assert_eq!(err.message, "This isn't a valid magnet link or torrent file.");
    assert!(store.list_downloads().await.unwrap().is_empty());
}

#[tokio::test]
async fn remove_during_metadata_drops_handle() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake) = start(dir.path()).await;
    fake.script(&hash(), FakeScript { meta: Some(meta()), metadata_after: None, ..Default::default() });
    let added = m.add_torrent(TorrentRequest { source: magnet(""), save_dir: None }).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    m.remove(&added.id, false).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fake.calls().contains(&FakeCall::FetchCancelled(hash())) {
        assert!(Instant::now() < deadline, "fetch not cancelled: {:?}", fake.calls());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(store.get_download(&added.id).await.unwrap().is_none());
    assert!(fake.live().is_empty());
}

#[tokio::test]
async fn speed_limit_reaches_torrents() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _store, fake) = start(dir.path()).await;
    m.set_speed_limit(500_000).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let hit = fake.calls().iter().any(|c| matches!(c, FakeCall::ApplySettings(s) if s.download_bps == Some(500_000)));
        if hit {
            break;
        }
        assert!(Instant::now() < deadline, "{:?}", fake.calls());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn restart_refetches_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let fake = FakeTorrentClient::new();
    fake.script(&hash(), FakeScript { meta: Some(meta()), metadata_after: None, ..Default::default() });
    let m = ManagerHandle::start_with_torrents(cfg(dir.path()), store.clone(), fake.clone()).await.unwrap();
    let added = m.add_torrent(TorrentRequest { source: magnet(""), save_dir: None }).await.unwrap();
    m.shutdown().await;

    let fake2 = FakeTorrentClient::new();
    fake2.script(&hash(), FakeScript { meta: Some(meta()), metadata_after: Some(Duration::ZERO), ..Default::default() });
    let m2 = ManagerHandle::start_with_torrents(cfg(dir.path()), store.clone(), fake2).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while m2.torrent_files(&added.id).await.unwrap().is_empty() {
        assert!(Instant::now() < deadline, "metadata not fetched after restart");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
