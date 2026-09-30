use clap::{Parser, Subcommand};
use odm_cli::{run_get, GetArgs};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "odm-cli", version, about = "ODM engine command-line driver")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Download a URL
    Get {
        url: String,
        #[arg(long)]
        dir: Option<PathBuf>,
        #[arg(long)]
        name: Option<String>,
        #[arg(short = 'c', long)]
        connections: Option<u32>,
        /// Speed limit in bytes per second (0 = unlimited)
        #[arg(long, default_value_t = 0)]
        limit: u64,
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        quiet: bool,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let Cmd::Get { url, dir, name, connections, limit, db, quiet } = Cli::parse().cmd;
    match run_get(GetArgs { url, dir, name, connections, limit_bps: limit, db, quiet }).await {
        Ok(p) => println!("saved {}", p.display()),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
