//! Engine tests for site (yt-dlp) downloads, run against the fake yt-dlp binary.
//!
//! They live here because `CARGO_BIN_EXE_fake-ytdlp` is only visible to this package's tests.
use odm_core::DownloadState;
use odm_engine::site::temp_dir;
use odm_engine::{EngineConfig, EngineEvent, ManagerHandle, SiteRequest};
use odm_store::{DownloadRecord, Store};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MISSING: &str = "yt-dlp not found \u{2014} install it or set its path in Settings.";

fn fake() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake-ytdlp"))
}

fn cfg(dir: &Path, ytdlp: Option<PathBuf>) -> EngineConfig {
    EngineConfig {
        download_dir: dir.to_path_buf(),
        progress_interval: Duration::from_millis(50),
        persist_interval: Duration::from_millis(200),
        ytdlp_path: ytdlp,
        ytdlp_search: false,
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

async fn start(out: &Path, ytdlp: Option<PathBuf>) -> ManagerHandle {
    ManagerHandle::start(cfg(out, ytdlp), Store::open_in_memory().unwrap())
        .await
        .unwrap()
}

fn site(out: &Path) -> SiteRequest {
    SiteRequest {
        page_url: "https://www.youtube.com/watch?v=abc".into(),
        save_dir: Some(out.to_path_buf()),
        ..Default::default()
    }
}

// The fake reads these from its environment, which the child inherits. Every test in this
// file takes this lock so the variables of one cannot reach another.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test(flavor = "multi_thread")]
async fn site_download_completes_with_fake_ytdlp() {
    let _g = ENV_LOCK.lock().await;
    let out = tempfile::tempdir().unwrap();
    let m = start(out.path(), Some(fake())).await;
    let mut events = m.subscribe();
    let id = m.add_site(site(out.path())).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed, 30).await;
    assert_eq!(r.kind, "site");
    assert_eq!(r.filename, "video [abc].mp4");
    let file = out.path().join("video [abc].mp4");
    assert_eq!(std::fs::metadata(&file).unwrap().len(), 300_000);
    assert!(!temp_dir(&r).exists());
    assert_eq!(r.size, Some(300_000));
    let (mut big_progress, mut merging) = (false, false);
    while let Ok(ev) = events.try_recv() {
        match ev {
            EngineEvent::Progress(p) if p.bytes > 200_000 => big_progress = true,
            EngineEvent::State {
                state: DownloadState::Merging,
                ..
            } => merging = true,
            _ => {}
        }
    }
    assert!(big_progress, "no Progress event above 200000 bytes");
    assert!(merging, "no Merging state");
    m.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn site_failure_reports_stderr_tail() {
    let _g = ENV_LOCK.lock().await;
    std::env::set_var("FAKE_YTDLP_FAIL", "1");
    let out = tempfile::tempdir().unwrap();
    let m = start(out.path(), Some(fake())).await;
    let id = m.add_site(site(out.path())).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Failed, 30).await;
    std::env::remove_var("FAKE_YTDLP_FAIL");
    let msg = r.error_msg.unwrap();
    let mut lines = msg.lines();
    assert_eq!(lines.next(), Some("yt-dlp could not download this video:"));
    let tail: Vec<&str> = lines.collect();
    assert_eq!(tail.len(), 20);
    assert_eq!(tail[0], "ERROR line 6");
    assert_eq!(tail[19], "ERROR line 25");
    m.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_ytdlp_fails_with_message() {
    let _g = ENV_LOCK.lock().await;
    let out = tempfile::tempdir().unwrap();
    let m = start(out.path(), Some(PathBuf::from("Z:/nope/yt-dlp.exe"))).await;
    let id = m.add_site(site(out.path())).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Failed, 30).await;
    assert_eq!(r.error_msg.as_deref(), Some(MISSING));
    m.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_site_kills_and_cleans() {
    let _g = ENV_LOCK.lock().await;
    std::env::set_var("FAKE_YTDLP_SLOW", "1");
    let out = tempfile::tempdir().unwrap();
    let m = start(out.path(), Some(fake())).await;
    let id = m.add_site(site(out.path())).await.unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Downloading, 30).await;
    let tmp = temp_dir(&r);
    assert!(tmp.exists());
    m.cancel(&id, true).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Cancelled, 30).await;
    std::env::remove_var("FAKE_YTDLP_SLOW");
    assert!(!tmp.exists(), "temp dir should be removed");
    assert!(!out.path().join("video [abc].mp4").exists());
    m.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn site_name_precedence_and_args() {
    let _g = ENV_LOCK.lock().await;
    let args_dir = tempfile::tempdir().unwrap();
    let args_file = args_dir.path().join("args.txt");
    std::env::set_var("FAKE_YTDLP_ARGS", &args_file);
    let out = tempfile::tempdir().unwrap();
    let m = start(out.path(), Some(fake())).await;
    let titled = SiteRequest {
        page_title: Some("Test: clip?".into()),
        headers: vec![("Cookie".into(), "a=1".into())],
        start_paused: true,
        ..site(out.path())
    };
    let id = m.add_site(titled).await.unwrap();
    let (rec, _) = m.get(&id).await.unwrap().unwrap();
    assert_eq!(
        (rec.kind.as_str(), rec.filename.as_str()),
        ("site", "Test clip.mp4")
    );
    assert_eq!(rec.category, "video");
    assert_eq!(rec.url, "https://www.youtube.com/watch?v=abc");
    let locked = SiteRequest {
        filename: Some("Mine".into()),
        start_paused: true,
        ..site(out.path())
    };
    let id2 = m.add_site(locked).await.unwrap();
    let (rec2, _) = m.get(&id2).await.unwrap().unwrap();
    assert_eq!(rec2.filename, "Mine.mp4");
    let bare = m
        .add_site(SiteRequest {
            start_paused: true,
            ..site(out.path())
        })
        .await
        .unwrap();
    assert_eq!(m.get(&bare).await.unwrap().unwrap().0.filename, "video.mp4");

    m.resume(&id2).await.unwrap();
    let done = wait_state(m.store(), &id2, DownloadState::Completed, 30).await;
    // The user locked a name, so it wins over the tool's own file name.
    assert_eq!(done.filename, "Mine.mp4");
    assert_eq!(
        std::fs::metadata(out.path().join("Mine.mp4"))
            .unwrap()
            .len(),
        300_000
    );
    m.resume(&id).await.unwrap();
    wait_state(m.store(), &id, DownloadState::Completed, 30).await;
    let args = std::fs::read_to_string(&args_file).unwrap();
    assert!(args.lines().any(|l| l == "Cookie:a=1"), "{args}");
    assert_eq!(
        args.lines().last(),
        Some("https://www.youtube.com/watch?v=abc")
    );
    std::env::remove_var("FAKE_YTDLP_ARGS");
    m.shutdown().await;
}

/// Downloads a real video with the installed yt-dlp. Run with `ODM_REAL_YTDLP=1`:
/// `cargo test -p odm-testserver --test site -- --ignored`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs yt-dlp, FFmpeg and network; set ODM_REAL_YTDLP=1"]
async fn real_ytdlp_downloads_a_playable_video() {
    if std::env::var_os("ODM_REAL_YTDLP").is_none() {
        eprintln!("ODM_REAL_YTDLP not set; skipping");
        return;
    }
    let _g = ENV_LOCK.lock().await;
    let out = tempfile::tempdir().unwrap();
    let config = EngineConfig {
        download_dir: out.path().to_path_buf(),
        progress_interval: Duration::from_millis(100),
        ..EngineConfig::default()
    };
    let m = ManagerHandle::start(config, Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let id = m
        .add_site(SiteRequest {
            page_url: "https://www.youtube.com/watch?v=jNQXAC9IVRw".into(),
            save_dir: Some(out.path().to_path_buf()),
            ..Default::default()
        })
        .await
        .unwrap();
    let r = wait_state(m.store(), &id, DownloadState::Completed, 240).await;
    let file = out.path().join(&r.filename);
    assert!(file.is_file(), "{file:?}");
    let probe = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(&file)
        .output()
        .expect("ffprobe on PATH");
    let secs: f64 = String::from_utf8_lossy(&probe.stdout)
        .trim()
        .parse()
        .unwrap();
    assert!((18.0..=20.0).contains(&secs), "duration {secs}");
    m.shutdown().await;
}
