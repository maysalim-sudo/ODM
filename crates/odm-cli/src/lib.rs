use odm_core::DownloadState;
use odm_engine::{
    AddRequest, ChunkView, ChunkViewState, EngineConfig, EngineEvent, ManagerHandle,
    ProgressSnapshot,
};
use odm_store::Store;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct GetArgs {
    pub url: String,
    pub dir: Option<PathBuf>,
    pub name: Option<String>,
    pub connections: Option<u32>,
    pub limit_bps: u64,
    pub db: Option<PathBuf>,
    pub quiet: bool,
}

fn default_db() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(odm_core::ids::APP_DIR_NAME).join("odm-cli.db")
}

fn human(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", U[i])
}

/// "1 connection" / "3 connections": chunks with a worker transferring them.
fn connections_label(chunks: &[ChunkView]) -> String {
    let n = chunks
        .iter()
        .filter(|c| c.state == ChunkViewState::Active)
        .count();
    if n == 1 {
        "1 connection".to_string()
    } else {
        format!("{n} connections")
    }
}

/// The carriage-return progress line printed while downloading.
fn progress_line(p: &ProgressSnapshot) -> String {
    let total = p.total.map(human).unwrap_or_else(|| "?".into());
    format!(
        "\r{} / {}  {}/s  {}   ",
        human(p.bytes),
        total,
        human(p.speed_bps),
        connections_label(&p.chunks)
    )
}

pub(crate) fn engine_config(dir: Option<&Path>, limit_bps: u64) -> EngineConfig {
    let default_cfg = EngineConfig::default();
    match dir {
        Some(d) => EngineConfig {
            speed_limit_bps: limit_bps,
            download_dir: d.to_path_buf(),
            ..default_cfg
        },
        None => EngineConfig {
            speed_limit_bps: limit_bps,
            ..default_cfg
        },
    }
}

pub async fn run_get(args: GetArgs) -> Result<PathBuf, String> {
    let store = Store::open(&args.db.unwrap_or_else(default_db)).map_err(|e| e.to_string())?;
    let cfg = engine_config(args.dir.as_deref(), args.limit_bps);
    let m = ManagerHandle::start(cfg, store.clone())
        .await
        .map_err(|e| e.to_string())?;
    let mut rx = m.subscribe();
    let mut req = AddRequest::new(args.url);
    req.filename = args.name;
    req.max_connections = args.connections;
    req.save_dir = args.dir;
    let id = m.add(req).await.map_err(|e| e.to_string())?;

    let result = loop {
        match rx.recv().await {
            Ok(EngineEvent::Progress(p)) if p.id == id && !args.quiet => {
                print!("{}", progress_line(&p));
                let _ = std::io::stdout().flush();
            }
            Ok(EngineEvent::State {
                id: i,
                state,
                error_code,
                error_message,
            }) if i == id => match state {
                DownloadState::Completed => {
                    let r = store
                        .get_download(&id)
                        .await
                        .map_err(|e| e.to_string())?
                        .ok_or("record vanished")?;
                    break Ok(Path::new(&r.save_dir).join(&r.filename));
                }
                DownloadState::Failed | DownloadState::Cancelled => {
                    break Err(format!(
                        "{}: {}",
                        error_code.unwrap_or_else(|| state.as_str().into()),
                        error_message.unwrap_or_default()
                    ));
                }
                _ => {}
            },
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            Err(e) => break Err(e.to_string()),
        }
    };
    if !args.quiet {
        println!();
    }
    m.shutdown().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(idx: u32, state: ChunkViewState) -> ChunkView {
        ChunkView {
            idx,
            start: 0,
            end: None,
            written: 0,
            state,
        }
    }

    #[test]
    fn progress_line_has_exact_format() {
        use ChunkViewState::*;
        let p = ProgressSnapshot {
            id: "a".into(),
            bytes: 5 * 1024 * 1024,
            total: Some(100 * 1024 * 1024),
            speed_bps: 2 * 1024 * 1024,
            eta_s: None,
            chunks: vec![view(0, Active), view(1, Active), view(2, Done)],
            segments: None,
            torrent: None,
        };
        assert_eq!(
            progress_line(&p),
            "\r5.0 MB / 100.0 MB  2.0 MB/s  2 connections   "
        );
    }

    #[test]
    fn connections_label_counts_only_active_chunks() {
        use ChunkViewState::*;
        assert_eq!(connections_label(&[]), "0 connections");
        assert_eq!(
            connections_label(&[view(0, Done), view(1, Pending), view(2, Retrying)]),
            "0 connections"
        );
        assert_eq!(
            connections_label(&[view(0, Active), view(1, Done), view(2, Pending)]),
            "1 connection"
        );
        assert_eq!(
            connections_label(&[
                view(0, Active),
                view(1, Active),
                view(2, Active),
                view(3, Done)
            ]),
            "3 connections"
        );
    }

    #[test]
    fn engine_config_with_no_dir_preserves_default() {
        let cfg = engine_config(None, 0);
        let default_cfg = EngineConfig::default();
        assert_eq!(cfg.download_dir, default_cfg.download_dir);
    }

    #[test]
    fn engine_config_with_dir_overrides() {
        let test_dir = Path::new("/custom/path");
        let cfg = engine_config(Some(test_dir), 0);
        assert_eq!(cfg.download_dir, test_dir.to_path_buf());
    }

    #[test]
    fn engine_config_preserves_speed_limit() {
        let limit_bps = 1024 * 1024;
        let cfg = engine_config(None, limit_bps);
        assert_eq!(cfg.speed_limit_bps, limit_bps);
    }
}
