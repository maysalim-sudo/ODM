//! Saving a download as MP3: plain files go through FFmpeg before they are placed.
use odm_core::DownloadState;
use odm_engine::config::Quality;
use odm_engine::{AddRequest, EngineConfig, ManagerHandle, VideoOverride};
use odm_store::{DownloadRecord, Store};
use odm_testserver::{Faults, TestServer};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SIZE: u64 = 64 * 1024;

fn cfg(dir: &Path, ffmpeg: PathBuf) -> EngineConfig {
    EngineConfig {
        download_dir: dir.to_path_buf(),
        progress_interval: Duration::from_millis(50),
        persist_interval: Duration::from_millis(200),
        proxy: odm_engine::config::ProxyConfig::Direct,
        ffmpeg_path: Some(ffmpeg),
        ffmpeg_search_path: false,
        ..EngineConfig::default()
    }
}

/// A stand-in FFmpeg: it writes "mp3" to its last argument, or fails.
fn stub_ffmpeg(dir: &Path, ok: bool) -> PathBuf {
    #[cfg(windows)]
    {
        let path = dir.join("ffmpeg.bat");
        let body = if ok {
            "@echo off\r\n:next\r\nif \"%~2\"==\"\" goto last\r\nshift\r\ngoto next\r\n:last\r\n> \"%~1\" echo mp3\r\n"
        } else {
            "@echo off\r\necho no audio stream 1>&2\r\nexit /b 1\r\n"
        };
        std::fs::write(&path, body).unwrap();
        path
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("ffmpeg");
        let body = if ok {
            "#!/bin/sh\nfor last; do :; done\necho mp3 > \"$last\"\n"
        } else {
            "#!/bin/sh\necho no audio stream >&2\nexit 1\n"
        };
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

fn mp3_request(url: String, dir: &Path) -> AddRequest {
    AddRequest {
        filename: Some("clip.mp4".into()),
        save_dir: Some(dir.to_path_buf()),
        video_override: Some(VideoOverride {
            quality: Some(Quality::Audio),
            ..VideoOverride::default()
        }),
        ..AddRequest::new(url)
    }
}

async fn wait_for(store: &Store, id: &str, want: DownloadState) -> DownloadRecord {
    let deadline = Instant::now() + Duration::from_secs(20);
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

fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plain_file_ends_up_as_an_mp3() {
    let tools = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let s = TestServer::start(SIZE, Faults::default()).await;
    let m = ManagerHandle::start(
        cfg(out.path(), stub_ffmpeg(tools.path(), true)),
        Store::open_in_memory().unwrap(),
    )
    .await
    .unwrap();
    let id = m
        .add(mp3_request(s.url("/file"), out.path()))
        .await
        .unwrap();
    let r = wait_for(m.store(), &id, DownloadState::Completed).await;
    assert_eq!(r.filename, "clip.mp3");
    assert_eq!(files_in(out.path()), vec!["clip.mp3".to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_taken_mp3_name_follows_the_exists_rule() {
    let tools = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    std::fs::write(out.path().join("clip.mp3"), "keep me").unwrap();
    let s = TestServer::start(SIZE, Faults::default()).await;
    let m = ManagerHandle::start(
        cfg(out.path(), stub_ffmpeg(tools.path(), true)),
        Store::open_in_memory().unwrap(),
    )
    .await
    .unwrap();
    let id = m
        .add(mp3_request(s.url("/file"), out.path()))
        .await
        .unwrap();
    let r = wait_for(m.store(), &id, DownloadState::Completed).await;
    assert_eq!(r.filename, "clip (1).mp3");
    assert_eq!(
        std::fs::read_to_string(out.path().join("clip.mp3")).unwrap(),
        "keep me"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_conversion_keeps_the_download() {
    let tools = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let s = TestServer::start(SIZE, Faults::default()).await;
    let m = ManagerHandle::start(
        cfg(out.path(), stub_ffmpeg(tools.path(), false)),
        Store::open_in_memory().unwrap(),
    )
    .await
    .unwrap();
    let id = m
        .add(mp3_request(s.url("/file"), out.path()))
        .await
        .unwrap();
    let r = wait_for(m.store(), &id, DownloadState::Failed).await;
    assert!(r.error_msg.unwrap().contains("no audio stream"));
    let kept = files_in(out.path());
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(
        std::fs::metadata(out.path().join(&kept[0])).unwrap().len(),
        SIZE
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn no_ffmpeg_says_so_in_the_words_the_app_looks_for() {
    let out = tempfile::tempdir().unwrap();
    let s = TestServer::start(SIZE, Faults::default()).await;
    let m = ManagerHandle::start(
        cfg(out.path(), "Z:/nope/ffmpeg.exe".into()),
        Store::open_in_memory().unwrap(),
    )
    .await
    .unwrap();
    let id = m
        .add(mp3_request(s.url("/file"), out.path()))
        .await
        .unwrap();
    let r = wait_for(m.store(), &id, DownloadState::Failed).await;
    assert!(r.error_msg.unwrap().starts_with("FFmpeg not found"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plain_download_never_needs_ffmpeg() {
    let out = tempfile::tempdir().unwrap();
    let s = TestServer::start(SIZE, Faults::default()).await;
    let m = ManagerHandle::start(
        cfg(out.path(), "Z:/nope/ffmpeg.exe".into()),
        Store::open_in_memory().unwrap(),
    )
    .await
    .unwrap();
    let id = m
        .add(AddRequest {
            filename: Some("clip.mp4".into()),
            ..AddRequest::new(s.url("/file"))
        })
        .await
        .unwrap();
    let r = wait_for(m.store(), &id, DownloadState::Completed).await;
    assert_eq!(r.filename, "clip.mp4");
}

#[tokio::test(flavor = "multi_thread")]
async fn try_again_after_a_failed_conversion_finishes_without_downloading_again() {
    let tools = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let s = TestServer::start(SIZE, Faults::default()).await;
    let m = ManagerHandle::start(
        cfg(out.path(), stub_ffmpeg(tools.path(), false)),
        Store::open_in_memory().unwrap(),
    )
    .await
    .unwrap();
    let id = m
        .add(mp3_request(s.url("/file"), out.path()))
        .await
        .unwrap();
    wait_for(m.store(), &id, DownloadState::Failed).await;
    stub_ffmpeg(tools.path(), true);
    m.resume(&id).await.unwrap();
    let r = wait_for(m.store(), &id, DownloadState::Completed).await;
    assert_eq!(r.filename, "clip.mp3");
    assert_eq!(files_in(out.path()), vec!["clip.mp3".to_string()]);
}
