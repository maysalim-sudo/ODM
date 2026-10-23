//! LibrqbitClient against a second librqbit session seeding on localhost.
use bytes::Bytes;
use librqbit::{CreateTorrentOptions, ListenerOptions, Session, SessionOptions};
use odm_torrent::rqbit::LibrqbitClient;
use odm_torrent::*;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

struct Seeder {
    _session: Arc<Session>,
    _dir: tempfile::TempDir,
    magnet: String,
    torrent: Bytes,
    hash: InfoHash,
    addr: SocketAddr,
    files: Vec<(String, Vec<u8>)>,
}

fn content(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
}

/// Seeds `files` (relative paths) as one multi-file torrent called "Pack".
async fn seed(files: &[(&str, usize)]) -> Seeder {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Pack");
    let mut written = Vec::new();
    for (i, (rel, len)) in files.iter().enumerate() {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let data = content(*len, i as u8);
        std::fs::write(&p, &data).unwrap();
        written.push((rel.to_string(), data));
    }
    let port = port::pick_port(0).unwrap();
    let session = Session::new_with_opts(
        dir.path().join("seed-default"),
        SessionOptions {
            dht: None,
            persistence: None,
            disable_trackers: true,
            disable_local_service_discovery: true,
            listen: Some(ListenerOptions {
                listen_addr: ([0, 0, 0, 0], port).into(),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let (created, _handle) = session
        .create_and_serve_torrent(
            &root,
            CreateTorrentOptions {
                name: Some("Pack"),
                piece_length: Some(16 * 1024),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let hash = InfoHash::parse(&created.info_hash().as_string()).unwrap();
    Seeder {
        magnet: created.as_magnet().to_string(),
        torrent: created.as_bytes().unwrap(),
        hash,
        addr: ([127, 0, 0, 1], port).into(),
        _session: session,
        _dir: dir,
        files: written,
    }
}

fn settings(state: &Path, peers: Vec<SocketAddr>) -> TorrentSettings {
    TorrentSettings {
        listen_port: 0,
        upnp: false,
        upload_bps: None,
        download_bps: None,
        state_dir: state.to_path_buf(),
        extra_peers: peers,
        offline: true,
        ..Default::default()
    }
}

fn start_req(s: &Seeder, out: &Path, only: Option<Vec<usize>>) -> StartTorrent {
    StartTorrent {
        info_hash: s.hash.clone(),
        source: TorrentSource::Bytes(s.torrent.clone()),
        save_dir: out.to_path_buf(),
        only_files: only,
        trackers: vec![],
        download_bps: None,
    }
}

async fn wait_finished(c: &LibrqbitClient, h: &InfoHash, secs: u64) -> TorrentStats {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(st) = c.stats(h) {
            if st.finished && st.has_meta && !st.checking {
                return st;
            }
        }
        assert!(tokio::time::Instant::now() < deadline, "not finished: {:?}", c.stats(h));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn file_index(s: &Seeder, meta: &TorrentMeta, rel: &str) -> usize {
    let _ = s;
    meta.files.iter().find(|f| f.path == rel).unwrap_or_else(|| panic!("{rel} in {:?}", meta.files)).idx
}

fn on_disk(out: &Path, rel: &str) -> Option<Vec<u8>> {
    std::fs::read(out.join(rel)).ok()
}

fn data<'a>(s: &'a Seeder, rel: &str) -> &'a [u8] {
    &s.files.iter().find(|(r, _)| r == rel).unwrap().1
}

#[tokio::test(flavor = "multi_thread")]
async fn magnet_fetches_metadata() {
    let s = seed(&[("a.bin", 40_000), ("sub/b.bin", 10_000)]).await;
    let state = tempfile::tempdir().unwrap();
    let c = LibrqbitClient::new(settings(state.path(), vec![s.addr]));
    let meta = tokio::time::timeout(
        Duration::from_secs(30),
        c.fetch_metadata(&TorrentSource::Magnet(s.magnet.clone()), CancellationToken::new()),
    )
    .await
    .expect("metadata in time")
    .unwrap();
    assert_eq!(meta.name, "Pack");
    let mut got: Vec<_> = meta.files.iter().map(|f| (f.path.clone(), f.size)).collect();
    got.sort();
    assert_eq!(got, vec![("a.bin".into(), 40_000), ("sub/b.bin".into(), 10_000)]);
    assert!(!meta.torrent_bytes.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn fetch_cancel_returns_cancelled() {
    let state = tempfile::tempdir().unwrap();
    // A peer that never answers: the fetch waits until cancelled.
    let dead: SocketAddr = ([127, 0, 0, 1], 9).into();
    let c = LibrqbitClient::new(settings(state.path(), vec![dead]));
    let cancel = CancellationToken::new();
    let c2 = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        c2.cancel();
    });
    let magnet = "magnet:?xt=urn:btih:abcdef0123456789abcdef0123456789abcdef01";
    let r = tokio::time::timeout(
        Duration::from_secs(5),
        c.fetch_metadata(&TorrentSource::Magnet(magnet.into()), cancel),
    )
    .await
    .expect("returns soon after cancel");
    assert_eq!(r, Err(TorrentError::Cancelled));
    let hash = InfoHash::parse("abcdef0123456789abcdef0123456789abcdef01").unwrap();
    assert!(c.stats(&hash).is_none(), "no handle left behind");
}

#[tokio::test(flavor = "multi_thread")]
async fn downloads_selected_files_only() {
    let s = seed(&[("a.bin", 50_000), ("b.bin", 70_000), ("c.bin", 30_000)]).await;
    let meta = parse(&TorrentSource::Bytes(s.torrent.clone())).unwrap().meta.unwrap();
    let (ia, ic) = (file_index(&s, &meta, "a.bin"), file_index(&s, &meta, "c.bin"));
    let state = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let c = LibrqbitClient::new(settings(state.path(), vec![s.addr]));
    c.start(start_req(&s, out.path(), Some(vec![ia, ic]))).await.unwrap();
    let st = wait_finished(&c, &s.hash, 60).await;
    assert_eq!(st.selected_total, 80_000);
    assert_eq!(st.selected_done, 80_000);
    assert_eq!(on_disk(out.path(), "a.bin").as_deref(), Some(data(&s, "a.bin")));
    assert_eq!(on_disk(out.path(), "c.bin").as_deref(), Some(data(&s, "c.bin")));
    let b = on_disk(out.path(), "b.bin");
    assert!(b.as_deref() != Some(data(&s, "b.bin")), "b.bin must not be fully downloaded");
}

#[tokio::test(flavor = "multi_thread")]
async fn set_files_mid_download() {
    let s = seed(&[("a.bin", 200_000), ("b.bin", 100_000)]).await;
    let meta = parse(&TorrentSource::Bytes(s.torrent.clone())).unwrap().meta.unwrap();
    let (ia, ib) = (file_index(&s, &meta, "a.bin"), file_index(&s, &meta, "b.bin"));
    let state = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let c = LibrqbitClient::new(settings(state.path(), vec![s.addr]));
    c.start(start_req(&s, out.path(), Some(vec![ia]))).await.unwrap();
    c.set_files(&s.hash, &[ia, ib]).await.unwrap();
    let st = wait_finished(&c, &s.hash, 60).await;
    assert_eq!(st.selected_total, 300_000);
    assert_eq!(on_disk(out.path(), "b.bin").as_deref(), Some(data(&s, "b.bin")));
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_stops_transfer() {
    let s = seed(&[("big.bin", 4_000_000)]).await;
    let state = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let mut set = settings(state.path(), vec![s.addr]);
    set.download_bps = Some(200_000);
    let c = LibrqbitClient::new(set);
    c.start(start_req(&s, out.path(), None)).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    c.pause(&s.hash).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let before = c.stats(&s.hash).unwrap().selected_done;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let after = c.stats(&s.hash).unwrap();
    assert_eq!(after.selected_done, before);
    assert!(!after.finished);
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_resumes_without_redownload() {
    let s = seed(&[("big.bin", 3_000_000)]).await;
    let state = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let mut set = settings(state.path(), vec![s.addr]);
    set.download_bps = Some(400_000);
    let c = LibrqbitClient::new(set.clone());
    c.start(start_req(&s, out.path(), None)).await.unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    let half = c.stats(&s.hash).unwrap().selected_done;
    assert!(half > 0 && half < 3_000_000, "partial: {half}");
    c.shutdown().await;
    drop(c);

    set.download_bps = None;
    let c = LibrqbitClient::new(set);
    c.restore().await.unwrap();
    // Restored torrents check their data, then wait for the engine's start.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while c.stats(&s.hash).expect("restored from persistence").checking {
        assert!(tokio::time::Instant::now() < deadline, "still checking");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let st = c.stats(&s.hash).unwrap();
    assert!(!st.finished);
    let idle = st.selected_done;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(c.stats(&s.hash).unwrap().selected_done, idle, "must not run before start");
    assert!(idle >= half.saturating_sub(16 * 1024 * 4), "kept progress: {idle} vs {half}");
    c.start(start_req(&s, out.path(), None)).await.unwrap();
    wait_finished(&c, &s.hash, 60).await;
    assert_eq!(on_disk(out.path(), "big.bin").as_deref(), Some(data(&s, "big.bin")));
}

#[tokio::test(flavor = "multi_thread")]
async fn remove_with_and_without_files() {
    let s = seed(&[("a.bin", 30_000)]).await;
    let state = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let c = LibrqbitClient::new(settings(state.path(), vec![s.addr]));
    c.start(start_req(&s, out.path(), None)).await.unwrap();
    wait_finished(&c, &s.hash, 60).await;
    c.remove(&s.hash, false).await.unwrap();
    assert!(c.stats(&s.hash).is_none());
    assert!(out.path().join("a.bin").exists(), "kept files");
    // Removing again is fine.
    c.remove(&s.hash, false).await.unwrap();

    c.start(start_req(&s, out.path(), None)).await.unwrap();
    wait_finished(&c, &s.hash, 60).await;
    c.remove(&s.hash, true).await.unwrap();
    assert!(!out.path().join("a.bin").exists(), "deleted files");
}

/// Names Windows cannot create are refused before librqbit sees them (parse.rs tests, with
/// hand-written torrents a seeder could not make). Ordinary unusual names download fine.
#[tokio::test(flavor = "multi_thread")]
async fn spaces_and_accents_download() {
    let s = seed(&[("Ünïcode dir/a b.txt", 5_000), ("ñ (1).txt", 5_000)]).await;
    let state = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let c = LibrqbitClient::new(settings(state.path(), vec![s.addr]));
    c.start(start_req(&s, out.path(), None)).await.unwrap();
    let st = wait_finished(&c, &s.hash, 60).await;
    assert_eq!(st.selected_done, 10_000);
    let mut names: Vec<PathBuf> = walk(out.path());
    names.sort();
    assert_eq!(names, vec![PathBuf::from("Ünïcode dir").join("a b.txt"), PathBuf::from("ñ (1).txt")]);
}

/// Every file under `root`, relative to it.
fn walk(root: &Path) -> Vec<PathBuf> {
    fn go(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                go(root, &p, out);
            } else {
                out.push(p.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    let mut out = Vec::new();
    go(root, root, &mut out);
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn pieces_map_fills() {
    let s = seed(&[("a.bin", 100_000)]).await;
    let state = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let c = LibrqbitClient::new(settings(state.path(), vec![s.addr]));
    c.start(start_req(&s, out.path(), None)).await.unwrap();
    wait_finished(&c, &s.hash, 60).await;
    let map = c.pieces(&s.hash).expect("piece map");
    assert_eq!(map.piece_count, 7); // 100_000 / 16 KiB, rounded up
    let set_bits: u32 = map.have.iter().map(|b| b.count_ones()).sum();
    assert_eq!(set_bits, 7);
}

#[tokio::test(flavor = "multi_thread")]
async fn remove_before_session_start_forgets_it() {
    let s = seed(&[("a.bin", 30_000)]).await;
    let state = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let c = LibrqbitClient::new(settings(state.path(), vec![s.addr]));
    c.start(start_req(&s, out.path(), None)).await.unwrap();
    wait_finished(&c, &s.hash, 60).await;
    c.shutdown().await;
    drop(c);
    // Next run: removed before anything started the session.
    let c = LibrqbitClient::new(settings(state.path(), vec![s.addr]));
    c.remove(&s.hash, false).await.unwrap();
    c.shutdown().await;
    drop(c);
    let c = LibrqbitClient::new(settings(state.path(), vec![s.addr]));
    c.restore().await.unwrap();
    assert!(c.stats(&s.hash).is_none(), "removed torrent came back from the saved session");
}

#[tokio::test(flavor = "multi_thread")]
async fn restored_torrent_uses_a_new_folder() {
    let s = seed(&[("a.bin", 30_000)]).await;
    let state = tempfile::tempdir().unwrap();
    let (out1, out2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let c = LibrqbitClient::new(settings(state.path(), vec![s.addr]));
    c.start(start_req(&s, out1.path(), None)).await.unwrap();
    wait_finished(&c, &s.hash, 60).await;
    c.pause(&s.hash).await.unwrap();
    c.shutdown().await;
    drop(c);
    let c = LibrqbitClient::new(settings(state.path(), vec![s.addr]));
    c.restore().await.unwrap();
    // Started again here first, so only the folder change can make it re-add.
    c.start(start_req(&s, out1.path(), None)).await.unwrap();
    wait_finished(&c, &s.hash, 60).await;
    c.pause(&s.hash).await.unwrap();
    c.start(start_req(&s, out2.path(), None)).await.unwrap();
    wait_finished(&c, &s.hash, 60).await;
    assert_eq!(on_disk(out2.path(), "a.bin").as_deref(), Some(data(&s, "a.bin")));
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_change_keeps_a_metadata_fetch_alive() {
    let state = tempfile::tempdir().unwrap();
    let dead: SocketAddr = ([127, 0, 0, 1], 9).into();
    let set = settings(state.path(), vec![dead]);
    let c = LibrqbitClient::new(set.clone());
    let cancel = CancellationToken::new();
    let (c2, cancel2) = (c.clone(), cancel.clone());
    let magnet = "magnet:?xt=urn:btih:abcdef0123456789abcdef0123456789abcdef01".to_string();
    let fetch = tokio::spawn(async move { c2.fetch_metadata(&TorrentSource::Magnet(magnet), cancel2).await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    let before = c.active_port().expect("session up for the fetch");
    let next = port::pick_port(0).unwrap();
    c.apply_settings(TorrentSettings { listen_port: next, ..set }).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    // The session the fetch uses keeps running.
    assert_eq!(c.active_port(), Some(before));
    assert!(!fetch.is_finished());
    cancel.cancel();
    assert_eq!(fetch.await.unwrap(), Err(TorrentError::Cancelled));
    // Idle again: the pending restart happens, and the next session uses the new port.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while c.active_port() == Some(before) {
        assert!(tokio::time::Instant::now() < deadline, "restart never happened");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    c.restore().await.unwrap();
    assert_eq!(c.active_port(), Some(next));
}

/// Bytes a file really takes on disk (less than its length when it is sparse).
#[cfg(windows)]
fn allocated(p: &Path) -> u64 {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetCompressedFileSizeW;
    let wide: Vec<u16> = p.as_os_str().encode_wide().chain([0]).collect();
    let mut hi = 0u32;
    let lo = unsafe { GetCompressedFileSizeW(wide.as_ptr(), &mut hi) };
    ((hi as u64) << 32) | lo as u64
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn unticked_files_take_no_disk_space() {
    let s = seed(&[("small.bin", 50_000), ("big.bin", 8_000_000)]).await;
    let meta = parse(&TorrentSource::Bytes(s.torrent.clone())).unwrap().meta.unwrap();
    let small = file_index(&s, &meta, "small.bin");
    let state = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let c = LibrqbitClient::new(settings(state.path(), vec![s.addr]));
    c.start(start_req(&s, out.path(), Some(vec![small]))).await.unwrap();
    wait_finished(&c, &s.hash, 60).await;
    let big = out.path().join("big.bin");
    let used = if big.exists() { allocated(&big) } else { 0 };
    assert!(used < 1_000_000, "the unticked 8 MB file takes {used} bytes on disk");
    assert_eq!(on_disk(out.path(), "small.bin").as_deref(), Some(data(&s, "small.bin")));
}
