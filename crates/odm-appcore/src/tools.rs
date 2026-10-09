//! The video tools ODM manages itself: yt-dlp, FFmpeg (with ffprobe) and Deno (YouTube's JS
//! runtime). They live in `AppPaths::tools_dir`; `install` fetches them over HTTPS and checks each
//! download against its published SHA-256 before anything is replaced.
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

pub const DOWNLOAD_FAILED: &str =
    "Could not download the video tools. Check your connection and try again.";
pub const CHECKSUM_FAILED: &str =
    "The downloaded file didn't match its checksum, so it wasn't installed.";

/// Versions of the tools found in the tools folder; `None` = not installed (or won't run).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ToolsStatus {
    pub ffmpeg: Option<String>,
    pub ytdlp: Option<String>,
    pub deno: Option<String>,
}

/// Where each tool and its checksum list are downloaded from.
#[derive(Debug, Clone)]
pub struct ToolSources {
    pub ytdlp_url: String,
    pub ytdlp_sums_url: String,
    pub ffmpeg_url: String,
    pub ffmpeg_sums_url: String,
    pub deno_url: String,
    pub deno_sums_url: String,
}

impl Default for ToolSources {
    fn default() -> Self {
        let ytdlp = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
        let ffmpeg = "https://github.com/yt-dlp/FFmpeg-Builds/releases/download/latest";
        let deno = "https://github.com/denoland/deno/releases/latest/download";
        Self {
            ytdlp_url: format!("{ytdlp}/yt-dlp.exe"),
            ytdlp_sums_url: format!("{ytdlp}/SHA2-256SUMS"),
            ffmpeg_url: format!("{ffmpeg}/ffmpeg-master-latest-win64-gpl.zip"),
            ffmpeg_sums_url: format!("{ffmpeg}/checksums.sha256"),
            deno_url: format!("{deno}/deno-x86_64-pc-windows-msvc.zip"),
            deno_sums_url: format!("{deno}/deno-x86_64-pc-windows-msvc.zip.sha256sum"),
        }
    }
}

/// The hash listed for `filename` in a `sha256sum`-style list (`<hash>  name` or `<hash> *name`).
/// A list that is just one bare hash applies to whatever file it belongs to.
pub fn find_checksum(list: &str, filename: &str) -> Option<String> {
    let is_hash = |t: &str| t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit());
    let mut bare = None;
    for line in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let mut parts = line.splitn(2, char::is_whitespace);
        let (Some(hash), name) = (parts.next(), parts.next()) else {
            continue;
        };
        if !is_hash(hash) {
            continue;
        }
        match name.map(|n| n.trim().trim_start_matches('*')) {
            Some(n) if n == filename => return Some(hash.to_ascii_lowercase()),
            Some(_) => {}
            None => bare = Some(hash.to_ascii_lowercase()),
        }
    }
    bare
}

/// The version number in a tool's version output.
pub fn parse_version(tool: &str, output: &str) -> Option<String> {
    let first = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    let words: Vec<&str> = first.split_whitespace().collect();
    match tool {
        // "ffmpeg version <v> Copyright ..." and "deno <v> (stable, ...)"
        "ffmpeg" => words.get(2).map(|v| v.to_string()),
        "deno" => words.get(1).map(|v| v.to_string()),
        _ => words.first().map(|v| v.to_string()),
    }
}

/// Whether a file last modified at `modified` is older than `max_age` at `now`.
pub fn is_stale(modified: SystemTime, now: SystemTime, max_age: Duration) -> bool {
    now.duration_since(modified).is_ok_and(|age| age > max_age)
}

const FFMPEG_EXE: &str = "ffmpeg.exe";
const YTDLP_EXE: &str = "yt-dlp.exe";
const DENO_EXE: &str = "deno.exe";

/// Runs `exe args` and returns its stdout, giving up (and killing it) after `timeout`.
fn run_with_timeout(exe: &Path, args: &[&str], timeout: Duration) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().ok()?;
    let out = child.stdout.take()?;
    // Read on a thread so a tool that prints a lot cannot block on a full pipe.
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.take(64 * 1024).read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let text = reader.join().ok()?;
    status.success().then_some(text)
}

/// The versions of the tools in `dir` (`ffmpeg.exe`, `yt-dlp.exe`, `deno.exe`). Each is run with
/// a 5 second limit; a missing or broken tool is `None`.
pub fn status(dir: &Path) -> ToolsStatus {
    let timeout = Duration::from_secs(5);
    let probe = |exe: &str, tool: &str, flag: &str| {
        let path = dir.join(exe);
        if !path.is_file() {
            return None;
        }
        parse_version(tool, &run_with_timeout(&path, &[flag], timeout)?)
    };
    ToolsStatus {
        ffmpeg: probe(FFMPEG_EXE, "ffmpeg", "-version"),
        ytdlp: probe(YTDLP_EXE, "ytdlp", "--version"),
        deno: probe(DENO_EXE, "deno", "--version"),
    }
}

fn other(detail: impl std::fmt::Display) -> String {
    format!("The video tools could not be installed: {detail}")
}

fn file_name_of(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.rsplit('/').next().unwrap_or(path).to_string()
}

async fn fetch_text(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|_| DOWNLOAD_FAILED.to_string())?;
    resp.text().await.map_err(|_| DOWNLOAD_FAILED.to_string())
}

/// An HTTP client for the tool downloads: no overall timeout (FFmpeg is about 150 MB).
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(format!("ODM/{}", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(60))
        .build()
        .unwrap_or_default()
}

type Progress<'a> = &'a (dyn Fn(&str, u64, u64) + Send + Sync);

/// Downloads `url` to `dest`, checking the SHA-256 listed for it at `sums_url`.
async fn download_verified(
    client: &reqwest::Client,
    url: &str,
    sums_url: &str,
    dest: &Path,
    stage: &str,
    on_progress: Progress<'_>,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    use std::io::Write;

    let sums = fetch_text(client, sums_url).await?;
    let expected = find_checksum(&sums, &file_name_of(url)).ok_or(CHECKSUM_FAILED)?;
    let resp = client
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|_| DOWNLOAD_FAILED.to_string())?;
    let total = resp.content_length().unwrap_or(0);
    let mut file = std::fs::File::create(dest).map_err(other)?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut last = None::<Instant>;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| DOWNLOAD_FAILED.to_string())?;
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(other)?;
        done += chunk.len() as u64;
        if last.is_none_or(|t| t.elapsed() >= Duration::from_millis(100)) {
            on_progress(stage, done, total);
            last = Some(Instant::now());
        }
    }
    file.flush().map_err(other)?;
    drop(file);
    on_progress(stage, done, done.max(total));
    if total > 0 && done != total {
        return Err(DOWNLOAD_FAILED.into());
    }
    let actual: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if actual != expected {
        return Err(CHECKSUM_FAILED.into());
    }
    Ok(())
}

/// Copies the zip entries whose path ends with one of `wanted` (`"bin/ffmpeg.exe"`) to
/// `<dir>/<file name>.part`, pushing each `.part` path to `scratch`; returns the (part, final)
/// pairs. Every wanted entry must exist.
fn extract(
    archive: &Path,
    wanted: &[&str],
    dir: &Path,
    scratch: &mut Vec<PathBuf>,
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let mut zip =
        zip::ZipArchive::new(std::fs::File::open(archive).map_err(other)?).map_err(other)?;
    let mut out = Vec::new();
    for want in wanted {
        let file_name = want.rsplit('/').next().unwrap_or(want);
        let index = (0..zip.len())
            .find(|&i| {
                zip.name_for_index(i).is_some_and(|n| {
                    let n = n.replace('\\', "/");
                    n == *want || n.ends_with(&format!("/{want}"))
                })
            })
            .ok_or_else(|| other(format!("{file_name} is missing from the download")))?;
        let part = dir.join(format!("{file_name}.part"));
        scratch.push(part.clone());
        let mut entry = zip.by_index(index).map_err(other)?;
        let mut f = std::fs::File::create(&part).map_err(other)?;
        std::io::copy(&mut entry, &mut f).map_err(other)?;
        out.push((part, dir.join(file_name)));
    }
    Ok(out)
}

/// Downloads yt-dlp, FFmpeg (+ ffprobe) and Deno into `dir`.
///
/// Everything is downloaded and checked first; only then are the files moved into place, so a
/// failure leaves the tools that were already there untouched. `on_progress(stage, done, total)`
/// is called while downloading (`total` is 0 when the server doesn't say).
pub async fn install(
    dir: &Path,
    client: &reqwest::Client,
    sources: &ToolSources,
    on_progress: Progress<'_>,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(other)?;
    let mut scratch: Vec<PathBuf> = Vec::new();
    let result = install_inner(dir, client, sources, on_progress, &mut scratch).await;
    for p in &scratch {
        let _ = std::fs::remove_file(p);
    }
    result
}

async fn install_inner(
    dir: &Path,
    client: &reqwest::Client,
    sources: &ToolSources,
    on_progress: Progress<'_>,
    scratch: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let part_of = |url: &str| dir.join(format!("{}.part", file_name_of(url)));

    let ytdlp = part_of(&sources.ytdlp_url);
    let ffmpeg_zip = part_of(&sources.ffmpeg_url);
    let deno_zip = part_of(&sources.deno_url);
    scratch.extend([ytdlp.clone(), ffmpeg_zip.clone(), deno_zip.clone()]);
    for (stage, url, sums, dest) in [
        (
            "yt-dlp",
            &sources.ytdlp_url,
            &sources.ytdlp_sums_url,
            &ytdlp,
        ),
        (
            "FFmpeg",
            &sources.ffmpeg_url,
            &sources.ffmpeg_sums_url,
            &ffmpeg_zip,
        ),
        ("Deno", &sources.deno_url, &sources.deno_sums_url, &deno_zip),
    ] {
        download_verified(client, url, sums, dest, stage, on_progress).await?;
    }

    on_progress("Installing", 0, 0);
    let mut moves = vec![(ytdlp, dir.join(YTDLP_EXE))];
    moves.extend(extract(
        &ffmpeg_zip,
        &["bin/ffmpeg.exe", "bin/ffprobe.exe"],
        dir,
        scratch,
    )?);
    moves.extend(extract(&deno_zip, &["deno.exe"], dir, scratch)?);
    for (part, dest) in moves {
        std::fs::rename(&part, &dest).map_err(|e| {
            other(format!(
                "{} is in use or can't be replaced ({e})",
                dest.display()
            ))
        })?;
    }
    on_progress("Installing", 1, 1);
    Ok(())
}

/// yt-dlp changes often. At app start, if the managed `yt-dlp.exe` is more than 7 days old, run
/// its own updater (`-U`) and log the outcome. A missing file does nothing.
pub async fn update_ytdlp_if_stale(dir: &Path) {
    let exe = dir.join(YTDLP_EXE);
    let Ok(modified) = std::fs::metadata(&exe).and_then(|m| m.modified()) else {
        return;
    };
    let week = Duration::from_secs(7 * 86_400);
    if !is_stale(modified, SystemTime::now(), week) {
        return;
    }
    tracing::info!("yt-dlp is older than 7 days; updating");
    let out = tokio::task::spawn_blocking(move || {
        run_with_timeout(&exe, &["-U"], Duration::from_secs(120))
    })
    .await;
    match out {
        Ok(Some(text)) => tracing::info!("yt-dlp update: {}", text.trim()),
        _ => tracing::warn!("yt-dlp update failed or timed out"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn checksum_lines_match_by_file_name() {
        let list = format!("{HASH_A}  yt-dlp\n{HASH_B}  yt-dlp.exe\n");
        assert_eq!(find_checksum(&list, "yt-dlp.exe").as_deref(), Some(HASH_B));
        let star = format!("{}  *ffmpeg.zip\r\n", HASH_A.to_uppercase());
        assert_eq!(find_checksum(&star, "ffmpeg.zip").as_deref(), Some(HASH_A));
        assert_eq!(find_checksum(&list, "missing.exe"), None);
    }

    #[test]
    fn a_bare_hash_applies_to_the_only_file() {
        assert_eq!(
            find_checksum(&format!("{HASH_A}\n"), "deno.zip").as_deref(),
            Some(HASH_A)
        );
        assert_eq!(find_checksum("not a hash", "deno.zip"), None);
    }

    #[test]
    fn versions_are_read_from_each_tools_output() {
        assert_eq!(
            parse_version("ytdlp", "2026.08.19\n").as_deref(),
            Some("2026.08.19")
        );
        assert_eq!(
            parse_version(
                "ffmpeg",
                "ffmpeg version N-118000-gabc Copyright (c) 2000-2026\nbuilt with gcc"
            )
            .as_deref(),
            Some("N-118000-gabc")
        );
        assert_eq!(
            parse_version(
                "deno",
                "deno 2.1.4 (stable, release, x86_64-pc-windows-msvc)\nv8 13.0"
            )
            .as_deref(),
            Some("2.1.4")
        );
        assert_eq!(parse_version("ytdlp", "  \n"), None);
    }

    #[test]
    fn stale_means_older_than_the_limit() {
        let now = SystemTime::now();
        let week = Duration::from_secs(7 * 86_400);
        assert!(is_stale(now - week - Duration::from_secs(1), now, week));
        assert!(!is_stale(now - Duration::from_secs(86_400), now, week));
        // A file dated in the future is not stale.
        assert!(!is_stale(now + Duration::from_secs(60), now, week));
    }

    #[test]
    fn nothing_is_installed_in_an_empty_folder() {
        let t = tempfile::tempdir().unwrap();
        assert_eq!(status(t.path()), ToolsStatus::default());
        assert_eq!(status(&t.path().join("missing")), ToolsStatus::default());
    }
}
