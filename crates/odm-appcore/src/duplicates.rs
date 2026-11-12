//! A browser click for something already in the list.
use odm_core::DownloadState;
use odm_engine::convert::wants_mp3;
use odm_store::DownloadRecord;

pub use odm_engine::convert::asks_mp3;

/// The row that already holds this address with the same MP3 or video choice. Failed and
/// cancelled rows don't count, so a click can try again; torrents are matched elsewhere.
pub fn duplicate_of<'a>(
    rows: &'a [DownloadRecord],
    url: &str,
    mp3: bool,
) -> Option<&'a DownloadRecord> {
    rows.iter().find(|r| {
        r.kind != "torrent"
            && r.url == url
            && wants_mp3(r) == mp3
            && !matches!(r.state, DownloadState::Failed | DownloadState::Cancelled)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use odm_engine::config::Quality;
    use odm_engine::VideoOverride;

    const URL: &str = "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8";

    fn row(id: &str, state: DownloadState, mp3: bool) -> DownloadRecord {
        let mut r = DownloadRecord::new_hls(id, URL, "x.mp4", "C:/dl");
        r.state = state;
        if mp3 {
            r.video_override = VideoOverride {
                quality: Some(Quality::Audio),
                ..VideoOverride::default()
            }
            .to_stored();
        }
        r
    }

    #[test]
    fn the_same_address_and_choice_is_a_duplicate() {
        let rows = [row("a", DownloadState::Downloading, false)];
        assert_eq!(
            duplicate_of(&rows, URL, false).map(|r| r.id.as_str()),
            Some("a")
        );
        let done = [row("a", DownloadState::Completed, true)];
        assert_eq!(
            duplicate_of(&done, URL, true).map(|r| r.id.as_str()),
            Some("a")
        );
    }

    #[test]
    fn mp3_after_video_is_a_different_file() {
        let rows = [row("a", DownloadState::Queued, false)];
        assert!(duplicate_of(&rows, URL, true).is_none());
    }

    #[test]
    fn failed_cancelled_and_other_addresses_dont_count() {
        let rows = [
            row("a", DownloadState::Failed, false),
            row("b", DownloadState::Cancelled, false),
        ];
        assert!(duplicate_of(&rows, URL, false).is_none());
        let rows = [row("a", DownloadState::Queued, false)];
        assert!(duplicate_of(&rows, "https://e.com/other.m3u8", false).is_none());
    }

    #[test]
    fn mp3_is_read_from_the_click() {
        assert!(!asks_mp3(None));
        assert!(!asks_mp3(Some(&VideoOverride::default())));
        assert!(asks_mp3(Some(&VideoOverride {
            quality: Some(Quality::Audio),
            ..VideoOverride::default()
        })));
    }
}

/// What the New download dialog shows about a file name: whether it's taken, and a free one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NameCheck {
    pub taken: bool,
    pub free: String,
}

/// Whether `name` is taken, by a file in `dir` or by another download's name (`others`, compared
/// without case, as Windows does), and the first free "name (1).ext" otherwise.
pub fn check_name(dir: &std::path::Path, name: &str, others: &[String]) -> NameCheck {
    let used = |n: &str| dir.join(n).exists() || others.iter().any(|o| o.eq_ignore_ascii_case(n));
    NameCheck {
        taken: used(name),
        free: odm_core::filename::unique_name(name, used),
    }
}

#[cfg(test)]
mod name_tests {
    use super::*;

    #[test]
    fn a_free_name_is_kept() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            check_name(d.path(), "video.mp4", &[]),
            NameCheck {
                taken: false,
                free: "video.mp4".into()
            }
        );
    }

    #[test]
    fn a_file_on_disk_or_another_download_takes_the_name() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("video.mp4"), "x").unwrap();
        assert_eq!(
            check_name(d.path(), "video.mp4", &["VIDEO (1).mp4".into()]),
            NameCheck {
                taken: true,
                free: "video (2).mp4".into()
            }
        );
        let empty = tempfile::tempdir().unwrap();
        assert!(check_name(empty.path(), "a.iso", &["A.ISO".into()]).taken);
    }
}

/// Free bytes on the drive that holds `dir`, or of its nearest existing parent (a folder ODM
/// will create); `None` when the system can't say.
pub fn free_space(dir: &std::path::Path) -> Option<u64> {
    let mut at = dir;
    while !at.exists() {
        at = at.parent()?;
    }
    free_space_at(at)
}

#[cfg(windows)]
fn free_space_at(dir: &std::path::Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut free: u64 = 0;
    // SAFETY: `wide` is a NUL-terminated path; the out pointer is a live u64.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free)
}

#[cfg(unix)]
fn free_space_at(dir: &std::path::Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a NUL-terminated path; `s` is a live statvfs.
    let ok = unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0;
    ok.then(|| s.f_bavail as u64 * s.f_frsize as u64)
}

#[cfg(test)]
mod space_tests {
    use super::free_space;

    #[test]
    fn free_space_is_read_for_a_folder_and_for_one_not_made_yet() {
        let d = tempfile::tempdir().unwrap();
        let here = free_space(d.path()).expect("free space");
        assert!(here > 0);
        assert!(free_space(&d.path().join("Videos").join("new")).is_some());
    }
}

/// A waiting plain file's size, asked of its server with the download's own headers, for the
/// New download dialog's "Later". `None` for streams and site videos (their size is only known
/// once they run), and when the server doesn't say.
pub async fn measure(
    m: &odm_engine::ManagerHandle,
    cfg: &odm_engine::EngineConfig,
    id: &str,
) -> Option<u64> {
    let (rec, _) = m.get(id).await.ok()??;
    if rec.kind != "file" {
        return None;
    }
    if rec.size.is_some() {
        return rec.size;
    }
    let headers = m.store().get_headers(id).await.ok()?;
    let client = odm_engine::download::build_client(cfg);
    let url = rec.final_url.as_deref().unwrap_or(&rec.url);
    odm_engine::probe::probe(&client, url, &odm_engine::download::header_map(&headers))
        .await
        .ok()?
        .size
}

#[cfg(test)]
mod measure_tests {
    use super::measure;
    use odm_engine::{AddRequest, EngineConfig, ManagerHandle};
    use odm_store::Store;
    use odm_testserver::{Faults, TestServer};

    #[tokio::test(flavor = "multi_thread")]
    async fn a_paused_file_gets_its_size_from_the_server() {
        let d = tempfile::tempdir().unwrap();
        let s = TestServer::start(123_456, Faults::default()).await;
        let cfg = EngineConfig {
            download_dir: d.path().to_path_buf(),
            ..Default::default()
        };
        let m = ManagerHandle::start(cfg.clone(), Store::open_in_memory().unwrap())
            .await
            .unwrap();
        let id = m
            .add(AddRequest {
                start_paused: true,
                ..AddRequest::new(s.url("/file"))
            })
            .await
            .unwrap();
        assert_eq!(measure(&m, &cfg, &id).await, Some(123_456));
        assert_eq!(measure(&m, &cfg, "gone").await, None);
    }
}
