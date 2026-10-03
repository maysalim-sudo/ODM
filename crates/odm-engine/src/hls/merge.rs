//! Merges downloaded HLS segments into one mp4 with FFmpeg.
use odm_core::{ErrorKind, OdmError};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio_util::sync::CancellationToken;

use crate::config::EngineConfig;

#[cfg(windows)]
const FFMPEG_EXE: &str = "ffmpeg.exe";
#[cfg(not(windows))]
const FFMPEG_EXE: &str = "ffmpeg";

/// The configured path if that file exists, else the first FFmpeg on `PATH`.
pub fn find_ffmpeg(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = configured.filter(|p| p.is_file()) {
        return Some(p.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(FFMPEG_EXE))
        .find(|c| c.is_file())
}

/// Like `find_ffmpeg`, but honours `EngineConfig::ffmpeg_search_path`.
pub fn resolve_ffmpeg(cfg: &EngineConfig) -> Option<PathBuf> {
    if cfg.ffmpeg_search_path {
        find_ffmpeg(cfg.ffmpeg_path.as_deref())
    } else {
        cfg.ffmpeg_path.clone().filter(|p| p.is_file())
    }
}

/// An FFmpeg concat list with bare file names: `file 'seg_000001.ts'` per line.
pub fn concat_list(n: u32, ext: &str) -> String {
    (1..=n)
        .map(|i| format!("file 'seg_{i:06}.{ext}'\n"))
        .collect()
}

/// The last `n` non-empty lines of `stderr`.
pub fn stderr_tail(stderr: &str, n: usize) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

fn merge_failed(detail: &str) -> OdmError {
    OdmError::new(
        ErrorKind::Other,
        format!("FFmpeg could not merge the video:\n{detail}"),
    )
}

/// Joins `init.mp4` and the fMP4 segments byte for byte into `joined.mp4`.
fn join_fmp4(dir: &Path, n: u32) -> std::io::Result<()> {
    use std::io::{copy, BufWriter};
    let mut out = BufWriter::new(std::fs::File::create(dir.join("joined.mp4"))?);
    copy(&mut std::fs::File::open(dir.join("init.mp4"))?, &mut out)?;
    for i in 1..=n {
        copy(
            &mut std::fs::File::open(dir.join(format!("seg_{i:06}.m4s")))?,
            &mut out,
        )?;
    }
    std::io::Write::flush(&mut out)
}

/// Runs FFmpeg in `dir`, producing `dir/out.mp4`. With `fmp4`, `init.mp4` and the
/// segments are first joined into `joined.mp4`, which is then remuxed.
pub async fn merge(
    ffmpeg: &Path,
    dir: &Path,
    n: u32,
    fmp4: bool,
    cancel: &CancellationToken,
) -> Result<PathBuf, OdmError> {
    let args: Vec<&str> = if fmp4 {
        let d = dir.to_path_buf();
        tokio::task::spawn_blocking(move || join_fmp4(&d, n))
            .await
            .map_err(|e| merge_failed(&e.to_string()))?
            .map_err(|e| merge_failed(&e.to_string()))?;
        vec![
            "-hide_banner",
            "-y",
            "-i",
            "joined.mp4",
            "-c",
            "copy",
            "-movflags",
            "+faststart",
            "out.mp4",
        ]
    } else {
        std::fs::write(dir.join("list.txt"), concat_list(n, "ts"))
            .map_err(|e| merge_failed(&e.to_string()))?;
        vec![
            "-hide_banner",
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            "list.txt",
            "-c",
            "copy",
            "-movflags",
            "+faststart",
            "out.mp4",
        ]
    };

    let mut cmd = tokio::process::Command::new(ffmpeg);
    cmd.args(&args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = cmd.spawn().map_err(|e| merge_failed(&e.to_string()))?;
    let mut stderr = child.stderr.take().expect("piped stderr");
    let reader = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf).await;
        String::from_utf8_lossy(&buf).into_owned()
    });

    let status = tokio::select! {
        _ = cancel.cancelled() => {
            let _ = child.kill().await;
            return Err(OdmError::new(ErrorKind::Cancelled, "cancelled"));
        }
        s = child.wait() => s.map_err(|e| merge_failed(&e.to_string()))?,
    };
    let log = reader.await.unwrap_or_default();
    let out = dir.join("out.mp4");
    if !status.success() || !out.is_file() {
        return Err(merge_failed(&stderr_tail(&log, 20)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concat_list_uses_bare_names() {
        assert_eq!(
            concat_list(2, "ts"),
            "file 'seg_000001.ts'\nfile 'seg_000002.ts'\n"
        );
    }

    #[test]
    fn stderr_tail_keeps_last_non_empty_lines() {
        assert_eq!(stderr_tail("a\n\nb\nc\n", 2), "b\nc");
        assert_eq!(stderr_tail("a\n", 5), "a");
        assert_eq!(stderr_tail("", 5), "");
    }

    #[test]
    fn find_ffmpeg_falls_back_to_path() {
        // Only asserts that a missing configured file neither panics nor is returned.
        let found = find_ffmpeg(Some(Path::new("Z:/nope/ffmpeg.exe")));
        assert_ne!(found, Some(PathBuf::from("Z:/nope/ffmpeg.exe")));
    }

    #[test]
    fn search_path_can_be_turned_off() {
        let cfg = EngineConfig {
            ffmpeg_path: Some("Z:/nope/ffmpeg.exe".into()),
            ffmpeg_search_path: false,
            ..EngineConfig::default()
        };
        assert_eq!(resolve_ffmpeg(&cfg), None);
    }
}
