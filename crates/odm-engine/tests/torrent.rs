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

// ---- running torrents ----

async fn wait_state(store: &Store, id: &str, want: DownloadState) -> DownloadRecord {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let r = row(store, id).await;
        if r.state == want {
            return r;
        }
        assert!(Instant::now() < deadline, "want {want:?}, have {:?} ({:?})", r.state, r.error_msg);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_call(fake: &FakeTorrentClient, f: impl Fn(&FakeCall) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fake.calls().iter().any(&f) {
        assert!(Instant::now() < deadline, "call not seen: {:?}", fake.calls());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn starts(fake: &FakeTorrentClient) -> Vec<Option<Vec<usize>>> {
    fake.calls()
        .into_iter()
        .filter_map(|c| match c {
            FakeCall::Start { only_files, .. } => Some(only_files),
            _ => None,
        })
        .collect()
}

async fn wait_files(m: &ManagerHandle, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while m.torrent_files(id).await.unwrap().is_empty() {
        assert!(Instant::now() < deadline, "no file list");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// A magnet whose metadata arrives at once, confirmed and started.
async fn running(dir: &Path, script: FakeScript) -> (ManagerHandle, Store, Arc<FakeTorrentClient>, String) {
    let (m, store, fake) = start(dir).await;
    fake.script(&hash(), FakeScript { meta: Some(meta()), metadata_after: Some(Duration::ZERO), ..script });
    let added = m.add_torrent(TorrentRequest { source: magnet(""), save_dir: None }).await.unwrap();
    wait_files(&m, &added.id).await;
    m.resume(&added.id).await.unwrap();
    (m, store, fake, added.id)
}

#[tokio::test]
async fn runs_to_completed_and_leaves_session() {
    let dir = tempfile::tempdir().unwrap();
    let (_m, store, fake, id) = running(dir.path(), FakeScript::default()).await;
    wait_state(&store, &id, DownloadState::Downloading).await;
    assert_eq!(starts(&fake), vec![Some(vec![0, 1])]);
    fake.advance(&hash(), 150);
    let r = wait_state(&store, &id, DownloadState::Completed).await;
    assert!(r.finished_at.is_some());
    let calls = fake.calls();
    assert_eq!(&calls[calls.len() - 2..], &[FakeCall::Pause(hash()), FakeCall::Remove { hash: hash(), delete: false }]);
    assert!(fake.live().is_empty());
    assert_eq!(store.get_with_bytes(&id).await.unwrap().unwrap().1, 150);
}

#[tokio::test]
async fn progress_never_decreases() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake, id) = running(dir.path(), FakeScript::default()).await;
    let mut rx = m.subscribe();
    wait_state(&store, &id, DownloadState::Downloading).await;
    let mut seen = Vec::new();
    for done in [50, 40, 60] {
        fake.advance(&hash(), done);
        let until = Instant::now() + Duration::from_millis(300);
        while let Ok(Ok(e)) = tokio::time::timeout(until.saturating_duration_since(Instant::now()), rx.recv()).await {
            if let EngineEvent::Progress(p) = e {
                if p.id == id {
                    assert!(p.torrent.is_some(), "torrent progress carries peers and upload");
                    seen.push(p.bytes);
                }
            }
        }
    }
    assert!(seen.windows(2).all(|w| w[0] <= w[1]), "went back: {seen:?}");
    assert!(seen.contains(&50) && seen.contains(&60), "{seen:?}");
    assert!(!seen.contains(&40), "{seen:?}");
}

#[tokio::test]
async fn set_files_live_calls_client() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake, id) = running(dir.path(), FakeScript::default()).await;
    wait_state(&store, &id, DownloadState::Downloading).await;
    m.set_torrent_files(&id, vec![1]).await.unwrap();
    wait_call(&fake, |c| *c == FakeCall::SetFiles(hash(), vec![1])).await;
    assert_eq!(row(&store, &id).await.size, Some(50));
}

#[tokio::test]
async fn set_files_rejects_empty() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _store, _fake, id) = running(dir.path(), FakeScript::default()).await;
    let err = m.set_torrent_files(&id, vec![]).await.unwrap_err();
    assert_eq!(err.message, "Keep at least one file selected.");
    assert_eq!(m.torrent_files(&id).await.unwrap().iter().filter(|f| f.selected).count(), 2);
}

#[tokio::test]
async fn retick_completed_requeues() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake) = start(dir.path()).await;
    fake.script(&hash(), FakeScript { meta: Some(meta()), metadata_after: Some(Duration::ZERO), ..Default::default() });
    let added = m.add_torrent(TorrentRequest { source: magnet(""), save_dir: None }).await.unwrap();
    wait_files(&m, &added.id).await;
    m.set_torrent_files(&added.id, vec![0]).await.unwrap();
    m.resume(&added.id).await.unwrap();
    wait_state(&store, &added.id, DownloadState::Downloading).await;
    fake.advance(&hash(), 100);
    wait_state(&store, &added.id, DownloadState::Completed).await;

    m.set_torrent_files(&added.id, vec![0, 1]).await.unwrap();
    wait_state(&store, &added.id, DownloadState::Downloading).await;
    assert_eq!(starts(&fake), vec![Some(vec![0]), Some(vec![0, 1])]);
}

#[tokio::test]
async fn unticking_on_completed_stays_completed() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake, id) = running(dir.path(), FakeScript::default()).await;
    wait_state(&store, &id, DownloadState::Downloading).await;
    fake.advance(&hash(), 150);
    wait_state(&store, &id, DownloadState::Completed).await;
    m.set_torrent_files(&id, vec![0]).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(row(&store, &id).await.state, DownloadState::Completed);
}

#[tokio::test]
async fn start_failure_fails_row() {
    let dir = tempfile::tempdir().unwrap();
    let script = FakeScript { fail_start: Some("disk full".into()), ..Default::default() };
    let (_m, store, _fake, id) = running(dir.path(), script).await;
    let r = wait_state(&store, &id, DownloadState::Failed).await;
    assert!(r.error_msg.unwrap_or_default().contains("disk full"));
}

#[tokio::test]
async fn http_downloads_unaffected_by_torrent_failure() {
    use odm_testserver::{body, Faults, TestServer};
    let dir = tempfile::tempdir().unwrap();
    let script = FakeScript { fail_start: Some("disk full".into()), ..Default::default() };
    let (m, store, _fake, id) = running(dir.path(), script).await;
    let s = TestServer::start(1 << 20, Faults::default()).await;
    let http = m.add(odm_engine::AddRequest::new(s.url("/file"))).await.unwrap();
    wait_state(&store, &id, DownloadState::Failed).await;
    let r = wait_state(&store, &http, DownloadState::Completed).await;
    assert_eq!(std::fs::read(Path::new(&r.save_dir).join(&r.filename)).unwrap(), body(1 << 20));
}

#[tokio::test]
async fn pause_then_resume_restarts_client() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake, id) = running(dir.path(), FakeScript::default()).await;
    wait_state(&store, &id, DownloadState::Downloading).await;
    m.pause(&id).await.unwrap();
    wait_state(&store, &id, DownloadState::Paused).await;
    assert!(fake.calls().contains(&FakeCall::Pause(hash())));
    m.resume(&id).await.unwrap();
    wait_state(&store, &id, DownloadState::Downloading).await;
    assert_eq!(starts(&fake).len(), 2);
}

#[tokio::test]
async fn cancel_deletes_files() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake, id) = running(dir.path(), FakeScript::default()).await;
    wait_state(&store, &id, DownloadState::Downloading).await;
    m.cancel(&id, false).await.unwrap();
    wait_state(&store, &id, DownloadState::Cancelled).await;
    wait_call(&fake, |c| *c == FakeCall::Remove { hash: hash(), delete: true }).await;
}

#[tokio::test]
async fn remove_keeps_or_deletes_files_as_asked() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake, id) = running(dir.path(), FakeScript::default()).await;
    wait_state(&store, &id, DownloadState::Downloading).await;
    m.remove(&id, false).await.unwrap();
    wait_call(&fake, |c| *c == FakeCall::Remove { hash: hash(), delete: false }).await;
    let deadline = Instant::now() + Duration::from_secs(5);
    while store.get_download(&id).await.unwrap().is_some() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // The stored .torrent copy goes with the row.
    assert!(!dir.path().join("state").join("files").join(format!("{HEX}.torrent")).exists());
}

#[tokio::test]
async fn download_all_before_metadata_fetches_it_first() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake) = start(dir.path()).await;
    fake.script(&hash(), FakeScript { meta: Some(meta()), metadata_after: None, ..Default::default() });
    let added = m.add_torrent(TorrentRequest { source: magnet(""), save_dir: None }).await.unwrap();
    // "Download all" while the list is still being fetched.
    fake.script(&hash(), FakeScript { meta: Some(meta()), metadata_after: Some(Duration::from_millis(100)), ..Default::default() });
    m.resume(&added.id).await.unwrap();
    wait_state(&store, &added.id, DownloadState::Probing).await;
    wait_state(&store, &added.id, DownloadState::Downloading).await;
    assert_eq!(starts(&fake), vec![Some(vec![0, 1])]);
    assert_eq!(m.torrent_files(&added.id).await.unwrap().len(), 2);
}

#[tokio::test]
async fn pieces_come_from_the_client() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, _fake, id) = running(dir.path(), FakeScript::default()).await;
    wait_state(&store, &id, DownloadState::Downloading).await;
    let p = m.pieces(&id).await.unwrap().unwrap();
    assert_eq!(p.piece_count, 8);
}

#[tokio::test]
async fn cancel_deletes_only_the_torrents_files() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, _fake, id) = running(dir.path(), FakeScript::default()).await;
    wait_state(&store, &id, DownloadState::Downloading).await;
    let out = Path::new(&row(&store, &id).await.save_dir).join("Foo");
    std::fs::create_dir_all(out.join("sub")).unwrap();
    std::fs::write(out.join("a.bin"), b"torrent").unwrap();
    std::fs::write(out.join("sub").join("b.bin"), b"torrent").unwrap();
    // The user's own file in a folder that happens to share the torrent's name.
    std::fs::write(out.join("mine.txt"), b"keep me").unwrap();
    m.cancel(&id, false).await.unwrap();
    wait_state(&store, &id, DownloadState::Cancelled).await;
    assert!(!out.join("a.bin").exists());
    assert!(!out.join("sub").exists(), "emptied folders go");
    assert_eq!(std::fs::read(out.join("mine.txt")).unwrap(), b"keep me");
}

#[tokio::test]
async fn cancelled_torrent_can_be_added_again() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, _fake, id) = running(dir.path(), FakeScript::default()).await;
    wait_state(&store, &id, DownloadState::Downloading).await;
    m.cancel(&id, false).await.unwrap();
    wait_state(&store, &id, DownloadState::Cancelled).await;
    let again = m.add_torrent(TorrentRequest { source: magnet(""), save_dir: None }).await.unwrap();
    assert!(!again.existing);
    assert_ne!(again.id, id);
    assert!(store.get_download(&id).await.unwrap().is_none(), "the cancelled row is replaced");
    assert_eq!(row(&store, &again.id).await.state, DownloadState::Paused);
}

#[tokio::test]
async fn torrent_file_completes_a_waiting_magnet() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake) = start(dir.path()).await;
    let bytes = torrent_bytes();
    let h = odm_torrent::parse(&TorrentSource::Bytes(bytes.clone())).unwrap().info_hash;
    fake.script(&h, FakeScript { metadata_after: None, ..Default::default() });
    let src = TorrentSource::Magnet(format!("magnet:?xt=urn:btih:{}", h.as_str()));
    let first = m.add_torrent(TorrentRequest { source: src, save_dir: None }).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let second = m.add_torrent(TorrentRequest { source: TorrentSource::Bytes(bytes), save_dir: None }).await.unwrap();
    assert!(second.existing);
    assert_eq!(second.id, first.id);
    assert_eq!(m.torrent_files(&first.id).await.unwrap().len(), 2);
    assert_eq!(row(&store, &first.id).await.size, Some(8));
    wait_call(&fake, |c| *c == FakeCall::FetchCancelled(h.clone())).await;
}

#[tokio::test]
async fn metadata_failure_is_shown() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake) = start(dir.path()).await;
    // The fetch ends with an error instead of a file list.
    fake.script(&hash(), FakeScript { meta: None, metadata_after: Some(Duration::ZERO), ..Default::default() });
    let added = m.add_torrent(TorrentRequest { source: magnet(""), save_dir: None }).await.unwrap();
    let r = wait_state(&store, &added.id, DownloadState::Failed).await;
    assert!(r.error_msg.unwrap_or_default().contains("no metadata scripted"));
}

#[tokio::test]
async fn fetched_names_windows_cannot_use_fail_the_row() {
    let dir = tempfile::tempdir().unwrap();
    let (m, store, fake) = start(dir.path()).await;
    let bad = TorrentMeta {
        name: "Foo".into(),
        files: vec![TorrentFile { idx: 0, path: "c:evil.dll".into(), size: 10 }],
        torrent_bytes: Bytes::new(),
    };
    fake.script(&hash(), FakeScript { meta: Some(bad), metadata_after: Some(Duration::ZERO), ..Default::default() });
    let added = m.add_torrent(TorrentRequest { source: magnet(""), save_dir: None }).await.unwrap();
    let r = wait_state(&store, &added.id, DownloadState::Failed).await;
    assert_eq!(r.error_msg.as_deref(), Some("This torrent has a file name Windows can't use: c:evil.dll"));
    assert!(m.torrent_files(&added.id).await.unwrap().is_empty());
}
