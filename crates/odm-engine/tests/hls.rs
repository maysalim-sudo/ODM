use odm_core::DownloadState;
use odm_engine::hls::merge::find_ffmpeg;
use odm_engine::hls::run::temp_dir;
use odm_engine::{EngineConfig, EngineEvent, ManagerHandle, StreamRequest};
use odm_store::{DownloadRecord, Store};
use odm_testserver::hls::{write_synthetic, HlsFaults, HlsServer};
use std::path::Path;
use std::time::{Duration, Instant};

fn cfg(dir: &Path) -> EngineConfig {
    EngineConfig {
        download_dir: dir.to_path_buf(),
        progress_interval: Duration::from_millis(50),
        persist_interval: Duration::from_millis(200),
        ..EngineConfig::default()
    }
}

async fn wait_state(store: &Store, id: &str, want: DownloadState, secs: u64) -> DownloadRecord {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let r = store.get_download(id).await.unwrap().expect("record");
        if r.state == want {
            return r;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {want:?}, state is {:?} ({:?})",
            r.state,
            r.error_msg
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn done_count(store: &Store, id: &str) -> usize {
    store
        .get_segments(id)
        .await
        .unwrap()
        .iter()
        .filter(|s| s.state == "done")
        .count()
}

struct Setup {
    _src: tempfile::TempDir,
    out: tempfile::TempDir,
    server: HlsServer,
    m: ManagerHandle,
}

/// A setup whose FFmpeg is unreachable, so the merge step fails the same way on every machine.
async fn setup(prepare: impl FnOnce(&Path), faults: HlsFaults) -> Setup {
    setup_with(prepare, faults, false).await
}

/// `real_ffmpeg`: use the FFmpeg on PATH instead of making it unreachable.
async fn setup_with(prepare: impl FnOnce(&Path), faults: HlsFaults, real_ffmpeg: bool) -> Setup {
    let src = tempfile::tempdir().unwrap();
    prepare(src.path());
    let out = tempfile::tempdir().unwrap();
    let server = HlsServer::start(src.path().to_path_buf(), faults).await;
    let mut config = cfg(out.path());
    if !real_ffmpeg {
        config.ffmpeg_path = Some("Z:/nope/ffmpeg.exe".into());
        config.ffmpeg_search_path = false;
    }
    let m = ManagerHandle::start(config, Store::open_in_memory().unwrap())
        .await
        .unwrap();
    Setup {
        _src: src,
        out,
        server,
        m,
    }
}

fn stream(url: String, out: &Path) -> StreamRequest {
    StreamRequest {
        playlist_url: url,
        save_dir: Some(out.to_path_buf()),
        ..Default::default()
    }
}

const NO_FFMPEG: &str = "FFmpeg not found — set its path in Settings.";

#[tokio::test(flavor = "multi_thread")]
async fn downloads_and_decrypts_all_segments() {
    let mut plains = Vec::new();
    let s = setup(
        |d| plains = write_synthetic(d, 12, 50_000, true),
        HlsFaults::default(),
    )
    .await;
    let id =
        s.m.add_stream(stream(s.server.url("/media.m3u8"), s.out.path()))
            .await
            .unwrap();
    let r = wait_state(s.m.store(), &id, DownloadState::Failed, 30).await;
    assert_eq!(r.error_msg.as_deref(), Some(NO_FFMPEG));
    let tmp = temp_dir(&r);
    for (i, plain) in plains.iter().enumerate() {
        let got = std::fs::read(tmp.join(format!("seg_{:06}.ts", i + 1))).unwrap();
        assert_eq!(&got, plain, "segment {}", i + 1);
    }
    assert_eq!(done_count(s.m.store(), &id).await, 12);
    assert_eq!(s.m.get(&id).await.unwrap().unwrap().1, 600_000);
}

#[tokio::test(flavor = "multi_thread")]
async fn master_playlist_picks_best_variant() {
    let s = setup(
        |d| {
            write_synthetic(d, 3, 1000, false);
            std::fs::write(
                d.join("master.m3u8"),
                "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\nlow.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=9\nmedia.m3u8\n",
            )
            .unwrap();
        },
        HlsFaults::default(),
    )
    .await;
    let id =
        s.m.add_stream(stream(s.server.url("/master.m3u8"), s.out.path()))
            .await
            .unwrap();
    wait_state(s.m.store(), &id, DownloadState::Failed, 30).await;
    let meta = s.m.store().get_hls_meta(&id).await.unwrap().unwrap();
    assert!(
        meta.variant_url.ends_with("media.m3u8"),
        "{}",
        meta.variant_url
    );
    assert_eq!(done_count(s.m.store(), &id).await, 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn live_and_sample_aes_fail_fast() {
    let s = setup(
        |d| {
            let seg = "#EXTINF:4.0,\ns1.ts\n";
            std::fs::write(d.join("s1.ts"), b"x").unwrap();
            std::fs::write(
                d.join("live.m3u8"),
                format!("#EXTM3U\n#EXT-X-TARGETDURATION:4\n{seg}"),
            )
            .unwrap();
            std::fs::write(
                d.join("sample.m3u8"),
                format!(
                    "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"key.bin\"\n{seg}#EXT-X-ENDLIST\n"
                ),
            )
            .unwrap();
        },
        HlsFaults::default(),
    )
    .await;
    let id =
        s.m.add_stream(stream(s.server.url("/live.m3u8"), s.out.path()))
            .await
            .unwrap();
    let r = wait_state(s.m.store(), &id, DownloadState::Failed, 15).await;
    assert_eq!(
        r.error_msg.as_deref(),
        Some("Live streams are not supported yet.")
    );

    let id =
        s.m.add_stream(stream(s.server.url("/sample.m3u8"), s.out.path()))
            .await
            .unwrap();
    let r = wait_state(s.m.store(), &id, DownloadState::Failed, 15).await;
    assert_eq!(
        r.error_msg.as_deref(),
        Some("This stream uses SAMPLE-AES encryption, which ODM can't download.")
    );
    assert_eq!(
        s.server
            .segment_requests
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_resume_skips_done_segments() {
    let mut plains = Vec::new();
    let s = setup(
        |d| plains = write_synthetic(d, 20, 100_000, false),
        HlsFaults {
            bytes_per_sec: Some(400_000),
            ..Default::default()
        },
    )
    .await;
    let max_conn = 8;
    let id =
        s.m.add_stream(stream(s.server.url("/media.m3u8"), s.out.path()))
            .await
            .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while done_count(s.m.store(), &id).await < 5 {
        assert!(Instant::now() < deadline, "no progress");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    s.m.pause(&id).await.unwrap();
    wait_state(s.m.store(), &id, DownloadState::Paused, 15).await;
    let done_before = done_count(s.m.store(), &id).await;
    assert!((5..20).contains(&done_before), "{done_before}");
    let requests_at_pause = s
        .server
        .segment_requests
        .load(std::sync::atomic::Ordering::SeqCst);

    s.m.resume(&id).await.unwrap();
    let r = wait_state(s.m.store(), &id, DownloadState::Failed, 30).await;
    assert_eq!(r.error_msg.as_deref(), Some(NO_FFMPEG));
    let total = s
        .server
        .segment_requests
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(total <= 20 + max_conn, "{total} requests");
    assert_eq!(
        total - requests_at_pause,
        (20 - done_before) as u32,
        "finished segments were fetched again"
    );
    let tmp = temp_dir(&r);
    for (i, plain) in plains.iter().enumerate() {
        let got = std::fs::read(tmp.join(format!("seg_{:06}.ts", i + 1))).unwrap();
        assert_eq!(&got, plain, "segment {}", i + 1);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn transient_segment_error_is_retried() {
    let s = setup(
        |d| {
            write_synthetic(d, 5, 2000, false);
        },
        HlsFaults {
            fail_segment_once: Some(3),
            ..Default::default()
        },
    )
    .await;
    let id =
        s.m.add_stream(stream(s.server.url("/media.m3u8"), s.out.path()))
            .await
            .unwrap();
    let r = wait_state(s.m.store(), &id, DownloadState::Failed, 30).await;
    assert_eq!(r.error_msg.as_deref(), Some(NO_FFMPEG));
    assert_eq!(done_count(s.m.store(), &id).await, 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn progress_reports_segments() {
    let s = setup(
        |d| {
            write_synthetic(d, 12, 20_000, false);
        },
        HlsFaults {
            bytes_per_sec: Some(400_000),
            ..Default::default()
        },
    )
    .await;
    let mut rx = s.m.subscribe();
    let id =
        s.m.add_stream(stream(s.server.url("/media.m3u8"), s.out.path()))
            .await
            .unwrap();
    wait_state(s.m.store(), &id, DownloadState::Failed, 30).await;
    let mut seen = None;
    while let Ok(e) = rx.try_recv() {
        if let EngineEvent::Progress(p) = e {
            if p.id == id {
                seen = Some(p);
            }
        }
    }
    let p = seen.expect("a progress event");
    let seg = p.segments.expect("segments");
    assert_eq!(seg.total, 12);
    assert!(p.chunks.is_empty());
}

fn have_ffmpeg() -> bool {
    let found = find_ffmpeg(None).is_some();
    if !found {
        eprintln!("skipped: ffmpeg not found");
    }
    found
}

/// Duration in seconds from ffprobe, or None when ffprobe is unavailable.
fn probe_duration(file: &Path) -> Option<f64> {
    let out = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(file)
        .output()
        .ok()?;
    assert!(out.status.success(), "ffprobe failed on {}", file.display());
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[tokio::test(flavor = "multi_thread")]
async fn real_stream_merges_to_playable_mp4() {
    if !have_ffmpeg() {
        return;
    }
    let s = setup_with(
        |d| assert!(odm_testserver::hls::write_real_ts(d), "write_real_ts"),
        HlsFaults::default(),
        true,
    )
    .await;
    let mut req = stream(s.server.url("/real/index.m3u8"), s.out.path());
    req.page_title = Some("Test: clip?".into());
    let id = s.m.add_stream(req).await.unwrap();
    let r = wait_state(s.m.store(), &id, DownloadState::Completed, 60).await;
    assert_eq!(r.filename, "Test clip.mp4");
    let out = s.out.path().join("Test clip.mp4");
    let len = std::fs::metadata(&out).unwrap().len();
    assert!(len > 10_000, "{len} bytes");
    if let Some(d) = probe_duration(&out) {
        assert!((3.5..=4.5).contains(&d), "duration {d}");
    } else {
        eprintln!("skipped: ffprobe not found");
    }
    assert!(!temp_dir(&r).exists(), "temp dir should be gone");
    assert_eq!(r.size, Some(len));
}

#[tokio::test(flavor = "multi_thread")]
async fn encrypted_real_stream_merges() {
    use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
    if !have_ffmpeg() {
        return;
    }
    let s = setup_with(
        |d| {
            assert!(odm_testserver::hls::write_real_ts(d), "write_real_ts");
            let real = d.join("real");
            let key: [u8; 16] = std::array::from_fn(|i| i as u8);
            std::fs::write(real.join("key.bin"), key).unwrap();
            let list = std::fs::read_to_string(real.join("index.m3u8")).unwrap();
            let mut seq = 0u64;
            for line in list.lines().filter(|l| l.ends_with(".ts")) {
                let plain = std::fs::read(real.join(line)).unwrap();
                let mut iv = [0u8; 16];
                iv[8..].copy_from_slice(&seq.to_be_bytes());
                let enc = cbc::Encryptor::<aes::Aes128>::new((&key).into(), (&iv).into())
                    .encrypt_padded_vec_mut::<Pkcs7>(&plain);
                std::fs::write(real.join(line), enc).unwrap();
                seq += 1;
            }
            assert!(seq >= 3, "expected several segments, got {seq}");
            // The key tag goes right after #EXT-X-TARGETDURATION.
            let rewritten = list
                .lines()
                .map(|l| {
                    if l.starts_with("#EXT-X-TARGETDURATION") {
                        format!("{l}\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"")
                    } else {
                        l.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            assert!(rewritten.contains("METHOD=AES-128"));
            std::fs::write(real.join("index.m3u8"), rewritten + "\n").unwrap();
        },
        HlsFaults::default(),
        true,
    )
    .await;
    let id =
        s.m.add_stream(stream(s.server.url("/real/index.m3u8"), s.out.path()))
            .await
            .unwrap();
    let r = wait_state(s.m.store(), &id, DownloadState::Completed, 60).await;
    let out = s.out.path().join(&r.filename);
    assert!(std::fs::metadata(&out).unwrap().len() > 10_000);
    if let Some(d) = probe_duration(&out) {
        assert!((3.5..=4.5).contains(&d), "duration {d}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_ffmpeg_fails_with_message() {
    let s = setup(
        |d| {
            write_synthetic(d, 3, 2000, false);
        },
        HlsFaults::default(),
    )
    .await;
    let id =
        s.m.add_stream(stream(s.server.url("/media.m3u8"), s.out.path()))
            .await
            .unwrap();
    let r = wait_state(s.m.store(), &id, DownloadState::Failed, 30).await;
    assert_eq!(r.error_msg.as_deref(), Some(NO_FFMPEG));
    let tmp = temp_dir(&r);
    for i in 1..=3 {
        assert!(
            tmp.join(format!("seg_{i:06}.ts")).exists(),
            "segment {i} kept"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn corrupt_segments_fail_with_ffmpeg_output() {
    if !have_ffmpeg() {
        return;
    }
    let s = setup_with(
        |d| {
            write_synthetic(d, 3, 4000, false);
        },
        HlsFaults::default(),
        true,
    )
    .await;
    let id =
        s.m.add_stream(stream(s.server.url("/media.m3u8"), s.out.path()))
            .await
            .unwrap();
    let r = wait_state(s.m.store(), &id, DownloadState::Failed, 60).await;
    let msg = r.error_msg.clone().unwrap();
    assert!(
        msg.starts_with("FFmpeg could not merge the video:"),
        "{msg}"
    );
    assert!(msg.lines().count() > 1, "stderr tail expected: {msg}");
    assert!(temp_dir(&r).join("seg_000001.ts").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_removes_temp_dir() {
    let s = setup(
        |d| {
            write_synthetic(d, 20, 100_000, false);
        },
        HlsFaults {
            bytes_per_sec: Some(200_000),
            ..Default::default()
        },
    )
    .await;
    let id =
        s.m.add_stream(stream(s.server.url("/media.m3u8"), s.out.path()))
            .await
            .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while done_count(s.m.store(), &id).await < 2 {
        assert!(Instant::now() < deadline, "no progress");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let rec = s.m.store().get_download(&id).await.unwrap().unwrap();
    let tmp = temp_dir(&rec);
    assert!(tmp.exists());
    s.m.cancel(&id, true).await.unwrap();
    wait_state(s.m.store(), &id, DownloadState::Cancelled, 15).await;
    assert!(!tmp.exists(), "temp dir should be removed on cancel");
}

#[tokio::test(flavor = "multi_thread")]
async fn real_fmp4_stream_merges_to_playable_mp4() {
    if !have_ffmpeg() {
        return;
    }
    let s = setup_with(
        |d| assert!(odm_testserver::hls::write_real_fmp4(d), "write_real_fmp4"),
        HlsFaults::default(),
        true,
    )
    .await;
    let id =
        s.m.add_stream(stream(s.server.url("/fmp4/index.m3u8"), s.out.path()))
            .await
            .unwrap();
    let r = wait_state(s.m.store(), &id, DownloadState::Completed, 60).await;
    let out = s.out.path().join(&r.filename);
    let len = std::fs::metadata(&out).unwrap().len();
    assert!(len > 10_000, "{len} bytes");
    if let Some(d) = probe_duration(&out) {
        assert!((3.5..=4.5).contains(&d), "duration {d}");
    } else {
        eprintln!("skipped: ffprobe not found");
    }
    assert!(!temp_dir(&r).exists(), "temp dir should be gone");
}
