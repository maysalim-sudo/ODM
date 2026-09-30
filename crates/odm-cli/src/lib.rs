use odm_core::DownloadState;
use odm_engine::{AddRequest, EngineConfig, EngineEvent, ManagerHandle};
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
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
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

pub async fn run_get(args: GetArgs) -> Result<PathBuf, String> {
    let store = Store::open(&args.db.unwrap_or_else(default_db)).map_err(|e| e.to_string())?;
    let cfg = EngineConfig {
        speed_limit_bps: args.limit_bps,
        download_dir: args.dir.clone().unwrap_or_default(),
        ..Default::default()
    };
    let m = ManagerHandle::start(cfg, store.clone()).await.map_err(|e| e.to_string())?;
    let mut rx = m.subscribe();
    let mut req = AddRequest::new(args.url);
    req.filename = args.name;
    req.max_connections = args.connections;
    req.save_dir = args.dir;
    let id = m.add(req).await.map_err(|e| e.to_string())?;

    let result = loop {
        match rx.recv().await {
            Ok(EngineEvent::Progress(p)) if p.id == id && !args.quiet => {
                let total = p.total.map(human).unwrap_or_else(|| "?".into());
                print!("\r{} / {}  {}/s  {} conns   ", human(p.bytes), total, human(p.speed_bps), p.chunks.len());
                let _ = std::io::stdout().flush();
            }
            Ok(EngineEvent::State { id: i, state, error_code, error_message }) if i == id => match state {
                DownloadState::Completed => {
                    let r = store.get_download(&id).await.map_err(|e| e.to_string())?.ok_or("record vanished")?;
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
