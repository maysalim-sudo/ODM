//! Runs yt-dlp for a site download and places the finished video.
use odm_core::{DownloadState, ErrorKind, OdmError};
use odm_store::{DownloadRecord, ProbeMeta, StoreError};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{build_args, find_node, find_ytdlp, parse_line, temp_dir, ByteCounter, Line};
use crate::download::{place_final, set_final_name, transition, DownloadEnv, Outcome};
use crate::events::{EngineEvent, ProgressSnapshot};
use crate::hls::merge::resolve_ffmpeg;
use crate::retry::io_err;

const MISSING: &str = "yt-dlp not found \u{2014} install it or set its path in Settings.";
const STDERR_LINES: usize = 20;

fn other(msg: impl Into<String>) -> OdmError {
    OdmError::new(ErrorKind::Other, msg)
}

fn store_err(e: StoreError) -> OdmError {
    other(format!("store: {e}"))
}

fn cancelled() -> OdmError {
    OdmError::new(ErrorKind::Cancelled, "cancelled")
}

pub async fn run_site(env: &DownloadEnv, id: &str, cancel: CancellationToken) -> Outcome {
    match run_inner(env, id, &cancel).await {
        Ok(path) => Outcome::Completed { path },
        Err(_) if cancel.is_cancelled() => Outcome::Stopped,
        Err(e) => Outcome::Failed(e),
    }
}

/// Reads `r` line by line (lossy UTF-8, so a stray byte never ends the stream).
async fn read_lines<R: AsyncRead + Unpin>(r: R, mut on_line: impl FnMut(String)) {
    let mut r = BufReader::new(r);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match r.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => on_line(String::from_utf8_lossy(&buf).trim_end().to_string()),
        }
    }
}

/// Stops yt-dlp and everything it started (the Windows build re-launches itself, and it runs FFmpeg).
async fn kill_tree(child: &mut tokio::process::Child) {
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        let mut cmd = tokio::process::Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let _ = cmd.status().await;
    }
    let _ = child.kill().await;
}

/// The smaller non-zero of the per-download and the global limit; 0 = unlimited.
fn limit_bps(rec: &DownloadRecord, global: u64) -> u64 {
    [rec.speed_limit.unwrap_or(0), global]
        .into_iter()
        .filter(|b| *b > 0)
        .min()
        .unwrap_or(0)
}

/// A finished video left in the temp dir when yt-dlp did not report its path.
fn find_output(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mp4")))
        .max_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

#[derive(Default)]
struct Latest {
    bytes: u64,
    total: Option<u64>,
    speed: Option<u64>,
    eta: Option<u64>,
}

fn emit(env: &DownloadEnv, id: &str, l: &Latest) {
    let _ = env.events.send(EngineEvent::Progress(ProgressSnapshot {
        id: id.to_string(),
        bytes: l.bytes,
        total: l.total,
        speed_bps: l.speed.unwrap_or(0),
        eta_s: l.eta,
        chunks: vec![],
        segments: None,
    }));
}

async fn run_inner(
    env: &DownloadEnv,
    id: &str,
    cancel: &CancellationToken,
) -> Result<PathBuf, OdmError> {
    let rec = env
        .store
        .get_download(id)
        .await
        .map_err(store_err)?
        .ok_or_else(|| other("download not found"))?;
    transition(env, id, DownloadState::Probing, None).await;
    let ytdlp = find_ytdlp(&env.config).ok_or_else(|| other(MISSING))?;
    let headers = env.store.get_headers(id).await.map_err(store_err)?;
    let tmp = temp_dir(&rec);
    std::fs::create_dir_all(&tmp).map_err(io_err)?;

    let ffmpeg = resolve_ffmpeg(&env.config);
    let node = find_node();
    let args = build_args(
        &rec,
        &headers,
        &tmp,
        ffmpeg.as_deref(),
        node.as_deref(),
        limit_bps(&rec, env.config.speed_limit_bps),
    );
    tracing::info!(%id, url = %odm_core::redact_url(&rec.url), "downloading with yt-dlp");

    let mut cmd = tokio::process::Command::new(&ytdlp);
    cmd.args(&args)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = cmd
        .spawn()
        .map_err(|e| other(format!("yt-dlp could not be started: {e}")))?;

    let (tx, mut rx) = mpsc::unbounded_channel::<Line>();
    let stdout = child.stdout.take().expect("piped stdout");
    let reader = tokio::spawn(read_lines(stdout, move |l| {
        let _ = tx.send(parse_line(&l));
    }));
    let tail: Arc<Mutex<VecDeque<String>>> = Arc::default();
    let stderr = child.stderr.take().expect("piped stderr");
    let tail_in = tail.clone();
    let err_reader = tokio::spawn(read_lines(stderr, move |l| {
        if l.trim().is_empty() {
            return;
        }
        let mut t = tail_in.lock().unwrap_or_else(|p| p.into_inner());
        if t.len() == STDERR_LINES {
            t.pop_front();
        }
        t.push_back(l);
    }));

    let mut counter = ByteCounter::default();
    let mut latest = Latest::default();
    let mut seen = false;
    let mut downloading = false;
    let mut merging = false;
    let mut output: Option<PathBuf> = None;
    let mut tick = tokio::time::interval(env.config.progress_interval);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                kill_tree(&mut child).await;
                return Err(cancelled());
            }
            _ = tick.tick() => {
                if seen {
                    emit(env, id, &latest);
                }
            }
            line = rx.recv() => match line {
                None => break,
                Some(Line::Progress { downloaded, total, speed, eta }) => {
                    let (bytes, total) = counter.update(downloaded, total);
                    latest = Latest { bytes, total, speed, eta };
                    seen = true;
                    if !downloading && !merging {
                        downloading = true;
                        transition(env, id, DownloadState::Downloading, None).await;
                    }
                }
                Some(Line::Merging) => {
                    if !merging {
                        merging = true;
                        latest.speed = None;
                        latest.eta = None;
                        transition(env, id, DownloadState::Merging, None).await;
                    }
                }
                Some(Line::Output(p)) => output = Some(p),
                Some(Line::Other) => {}
            },
        }
    }
    if seen {
        emit(env, id, &latest);
    }

    let status = tokio::select! {
        _ = cancel.cancelled() => {
            kill_tree(&mut child).await;
            return Err(cancelled());
        }
        s = child.wait() => s.map_err(|e| other(format!("yt-dlp could not be waited for: {e}")))?,
    };
    let _ = reader.await;
    let _ = err_reader.await;

    if !status.success() {
        let tail: Vec<String> = tail
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .cloned()
            .collect();
        return Err(other(format!(
            "yt-dlp could not download this video:\n{}",
            tail.join("\n")
        )));
    }
    let out = output
        .filter(|p| p.is_file())
        .or_else(|| find_output(&tmp))
        .ok_or_else(|| other("yt-dlp finished without producing a video file."))?;
    // A name the user chose in the dialog is kept; otherwise the tool's own file name is used.
    let name = if rec.filename_locked {
        rec.filename.clone()
    } else {
        out.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| rec.filename.clone())
    };
    let final_path = place_final(&out, Path::new(&rec.save_dir), &name, &rec)?;
    set_final_name(env, &rec, &final_path).await?;
    let len = std::fs::metadata(&final_path).map_err(io_err)?.len();
    env.store
        .update_probe(
            id,
            ProbeMeta {
                size: Some(len),
                ..Default::default()
            },
        )
        .await
        .map_err(store_err)?;
    if let Err(e) = std::fs::remove_dir_all(&tmp) {
        tracing::warn!(%id, "could not remove the site temp dir: {e}");
    }
    Ok(final_path)
}
