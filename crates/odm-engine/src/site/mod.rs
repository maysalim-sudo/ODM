//! Site videos (YouTube, Facebook, Instagram…) downloaded by the external tool yt-dlp.
//! Like FFmpeg, yt-dlp is never bundled: ODM finds it on the machine.
pub mod progress;
pub mod run;

pub use progress::{parse_line, ByteCounter, Line};
pub use run::run_site;

use crate::config::EngineConfig;
use crate::hls::{title_to_stem, with_mp4};
use odm_core::filename;
use odm_store::DownloadRecord;
use std::path::{Path, PathBuf};

/// Sites whose pages ODM hands to yt-dlp. A host matches itself and its subdomains.
pub const SITE_HOSTS: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "facebook.com",
    "fb.watch",
    "instagram.com",
    "x.com",
    "twitter.com",
    "tiktok.com",
    "vimeo.com",
    "dailymotion.com",
    "twitch.tv",
    "reddit.com",
];

/// Whether `host` is one of `SITE_HOSTS` or a subdomain of one.
pub fn is_site_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    SITE_HOSTS
        .iter()
        .any(|h| host == *h || host.strip_suffix(h).is_some_and(|rest| rest.ends_with('.')))
}

/// Whether `url` is an http(s) page on a site yt-dlp handles.
pub fn is_site_page_url(url: &str) -> bool {
    reqwest::Url::parse(url.trim())
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .and_then(|u| u.host_str().map(is_site_host))
        .unwrap_or(false)
}

#[cfg(windows)]
const YTDLP_EXE: &str = "yt-dlp.exe";
#[cfg(not(windows))]
const YTDLP_EXE: &str = "yt-dlp";

#[cfg(windows)]
const NODE_EXE: &str = "node.exe";
#[cfg(not(windows))]
const NODE_EXE: &str = "node";

fn on_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(exe))
        .find(|c| c.is_file())
}

/// The configured yt-dlp, else (when searching is on) `PATH`, the WinGet links folder and the
/// first WinGet `yt-dlp.yt-dlp_*` package folder.
pub fn find_ytdlp(cfg: &EngineConfig) -> Option<PathBuf> {
    if let Some(p) = cfg.ytdlp_path.as_ref().filter(|p| p.is_file()) {
        return Some(p.clone());
    }
    if !cfg.ytdlp_search {
        return None;
    }
    if let Some(p) = on_path(YTDLP_EXE) {
        return Some(p);
    }
    let winget = PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
        .join("Microsoft")
        .join("WinGet");
    let link = winget.join("Links").join(YTDLP_EXE);
    if link.is_file() {
        return Some(link);
    }
    let mut packages: Vec<PathBuf> = std::fs::read_dir(winget.join("Packages"))
        .ok()?
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("yt-dlp.yt-dlp_")
        })
        .map(|e| e.path().join(YTDLP_EXE))
        .collect();
    packages.sort();
    packages.into_iter().find(|p| p.is_file())
}

/// Node.js, which yt-dlp uses to solve YouTube's signature challenges.
pub fn find_node() -> Option<PathBuf> {
    on_path(NODE_EXE).or_else(|| {
        Some(PathBuf::from("C:\\Program Files\\nodejs\\node.exe")).filter(|p| p.is_file())
    })
}

/// `<save_dir>/.odm-site-<id>`: where yt-dlp works until the video is done.
pub fn temp_dir(rec: &DownloadRecord) -> PathBuf {
    Path::new(&rec.save_dir).join(format!(".odm-site-{}", rec.id))
}

/// The yt-dlp command line for a site download (everything after the executable).
pub fn build_args(
    rec: &DownloadRecord,
    headers: &[(String, String)],
    tmp: &Path,
    ffmpeg: Option<&Path>,
    node: Option<&Path>,
    limit_bps: u64,
) -> Vec<String> {
    let template = tmp.join("%(title).150B [%(id)s].%(ext)s");
    let mut a: Vec<String> = [
        "--newline",
        "--no-playlist",
        "--no-colors",
        "--continue",
        "--no-mtime",
        "-f",
        "bv*+ba/b",
        "--merge-output-format",
        "mp4",
        "-o",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.push(template.to_string_lossy().into_owned());
    a.push("--progress-template".into());
    a.push("download:ODMP %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s".into());
    a.push("--print".into());
    a.push("after_move:ODMFILE %(filepath)s".into());
    if let Some(f) = ffmpeg {
        a.push("--ffmpeg-location".into());
        a.push(f.to_string_lossy().into_owned());
    }
    if let Some(n) = node {
        a.push("--js-runtimes".into());
        a.push(format!("node:{}", n.to_string_lossy()));
    }
    if limit_bps > 0 {
        a.push("--limit-rate".into());
        a.push(limit_bps.to_string());
    }
    for (name, value) in headers {
        a.push("--add-header".into());
        a.push(format!("{name}:{value}"));
    }
    a.push(rec.url.clone());
    a
}

/// The file name shown for a site video before yt-dlp has named it: the requested name, else
/// the page title, else `video.mp4`. Always ends in `.mp4`.
pub fn output_name(requested: Option<&str>, page_title: Option<&str>) -> String {
    if let Some(n) = requested.filter(|n| !n.trim().is_empty()) {
        return with_mp4(&filename::sanitize(n));
    }
    if let Some(t) = page_title.filter(|t| !t.trim().is_empty()) {
        return with_mp4(&title_to_stem(t));
    }
    "video.mp4".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EngineConfig;
    use odm_store::DownloadRecord;
    use std::path::{Path, PathBuf};

    fn rec() -> DownloadRecord {
        DownloadRecord::new_site(
            "id1",
            "https://www.youtube.com/watch?v=abc",
            "v.mp4",
            "D:/out",
        )
    }

    #[test]
    fn find_ytdlp_prefers_the_configured_file() {
        let dir = std::env::temp_dir().join(format!("odm-ytdlp-find-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("yt-dlp.exe");
        std::fs::write(&exe, b"x").unwrap();
        let cfg = EngineConfig {
            ytdlp_path: Some(exe.clone()),
            ytdlp_search: false,
            ..EngineConfig::default()
        };
        assert_eq!(find_ytdlp(&cfg), Some(exe));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_ytdlp_missing_file_without_search_is_none() {
        let cfg = EngineConfig {
            ytdlp_path: Some(PathBuf::from("Z:/nope/yt-dlp.exe")),
            ytdlp_search: false,
            ..EngineConfig::default()
        };
        assert_eq!(find_ytdlp(&cfg), None);
    }

    fn base_args(tmp: &Path) -> Vec<String> {
        let template = tmp.join("%(title).150B [%(id)s].%(ext)s");
        [
            "--newline",
            "--no-playlist",
            "--no-colors",
            "--continue",
            "--no-mtime",
            "-f",
            "bv*+ba/b",
            "--merge-output-format",
            "mp4",
            "-o",
        ]
        .iter()
        .map(|s| s.to_string())
        .chain([
            template.to_string_lossy().into_owned(),
            "--progress-template".into(),
            "download:ODMP %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s".into(),
            "--print".into(),
            "after_move:ODMFILE %(filepath)s".into(),
        ])
        .collect()
    }

    #[test]
    fn build_args_with_everything() {
        let tmp = Path::new("D:/out/.odm-site-id1");
        let headers = vec![
            ("Cookie".to_string(), "a=b".to_string()),
            ("Referer".to_string(), "https://r/".to_string()),
        ];
        let got = build_args(
            &rec(),
            &headers,
            tmp,
            Some(Path::new("C:/ff/ffmpeg.exe")),
            Some(Path::new("C:/n/node.exe")),
            1024,
        );
        let mut want = base_args(tmp);
        want.extend(
            [
                "--ffmpeg-location",
                "C:/ff/ffmpeg.exe",
                "--js-runtimes",
                "node:C:/n/node.exe",
                "--limit-rate",
                "1024",
                "--add-header",
                "Cookie:a=b",
                "--add-header",
                "Referer:https://r/",
                "https://www.youtube.com/watch?v=abc",
            ]
            .iter()
            .map(|s| s.to_string()),
        );
        assert_eq!(got, want);
    }

    #[test]
    fn build_args_without_the_optional_parts() {
        let tmp = Path::new("D:/out/.odm-site-id1");
        let got = build_args(&rec(), &[], tmp, None, None, 0);
        let mut want = base_args(tmp);
        want.push("https://www.youtube.com/watch?v=abc".into());
        assert_eq!(got, want);
    }

    #[test]
    fn temp_dir_is_hidden_next_to_the_target() {
        assert_eq!(temp_dir(&rec()), Path::new("D:/out").join(".odm-site-id1"));
    }

    #[test]
    fn site_hosts_match_hosts_and_subdomains() {
        assert!(is_site_host("youtube.com"));
        assert!(is_site_host("m.youtube.com"));
        assert!(is_site_host("www.facebook.com"));
        assert!(is_site_host("youtu.be"));
        assert!(!is_site_host("notyoutube.com"));
        assert!(!is_site_host("example.com"));
        assert!(is_site_page_url("https://www.youtube.com/watch?v=abc"));
        assert!(!is_site_page_url("https://example.com/watch?v=abc"));
        assert!(!is_site_page_url("not a url"));
    }
}
