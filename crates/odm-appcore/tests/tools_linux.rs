//! Linux: `tools::install` fetches only yt-dlp (FFmpeg comes from the package manager), from a
//! local file server. Nothing here touches the real network.
#![cfg(unix)]
use odm_appcore::tools::{self, ToolSources, CHECKSUM_FAILED};
use odm_engine::EngineConfig;
use odm_testserver::files::FileServer;
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Runs like `yt-dlp --version`, so `tools::status` can read a version from it.
const FAKE_YTDLP: &[u8] = b"#!/bin/sh\necho 2099.01.01\n";

fn sha(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

async fn serve(body: &[u8], listed: &[u8]) -> (FileServer, ToolSources) {
    let asset = tools::ytdlp_asset();
    let sums = format!("{}  yt-dlp.exe\n{}  {asset}\n", sha(b"other"), sha(listed));
    let server = FileServer::start(
        vec![
            (format!("yt/{asset}"), body.to_vec()),
            ("yt/SHA2-256SUMS".into(), sums.into_bytes()),
        ],
        Vec::new(),
    )
    .await;
    let sources = ToolSources {
        ytdlp_url: server.url(&format!("yt/{asset}")),
        ytdlp_sums_url: server.url("yt/SHA2-256SUMS"),
        // Not fetched on Linux.
        ffmpeg_url: server.url("missing"),
        ffmpeg_sums_url: server.url("missing"),
        deno_url: server.url("missing"),
        deno_sums_url: server.url("missing"),
    };
    (server, sources)
}

fn leftovers(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".part"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn the_standalone_linux_build_is_chosen() {
    let asset = tools::ytdlp_asset();
    assert!(asset.starts_with("yt-dlp_linux"), "{asset}");
    assert!(ToolSources::default()
        .ytdlp_url
        .ends_with(&format!("/{asset}")));
}

#[tokio::test]
async fn installs_an_executable_yt_dlp_that_odm_then_prefers() {
    let (_server, sources) = serve(FAKE_YTDLP, FAKE_YTDLP).await;
    let dir = tempfile::tempdir().unwrap();
    let tools_dir = dir.path().join("tools");
    tools::install(&tools_dir, &reqwest::Client::new(), &sources, &|_, _, _| {})
        .await
        .unwrap();
    let exe = tools_dir.join("yt-dlp");
    assert_eq!(std::fs::read(&exe).unwrap(), FAKE_YTDLP);
    assert_eq!(
        std::fs::metadata(&exe).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert!(!tools_dir.join("ffmpeg").exists());
    assert!(
        leftovers(&tools_dir).is_empty(),
        "{:?}",
        leftovers(&tools_dir)
    );
    // ODM's copy wins over any yt-dlp on PATH.
    let status = tools::status(&EngineConfig {
        tools_dir: Some(tools_dir),
        ytdlp_search: true,
        ..EngineConfig::default()
    });
    let ytdlp = status.ytdlp.expect("yt-dlp found");
    assert_eq!(
        (ytdlp.version.as_str(), ytdlp.source.as_str()),
        ("2099.01.01", "ODM")
    );
}

#[tokio::test]
async fn a_checksum_mismatch_keeps_the_old_yt_dlp() {
    let (_server, sources) = serve(b"tampered", FAKE_YTDLP).await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("yt-dlp"), "old yt-dlp").unwrap();
    let err = tools::install(dir.path(), &reqwest::Client::new(), &sources, &|_, _, _| {})
        .await
        .unwrap_err();
    assert_eq!(err, CHECKSUM_FAILED);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("yt-dlp")).unwrap(),
        "old yt-dlp"
    );
    assert!(
        leftovers(dir.path()).is_empty(),
        "{:?}",
        leftovers(dir.path())
    );
}

/// Downloads the real yt-dlp from GitHub (about 35 MB) and checks that it runs. Opt-in:
/// `ODM_REAL_TOOLS=1 cargo test -p odm-appcore --test tools_linux -- --ignored`.
#[tokio::test]
#[ignore = "downloads ~35 MB from GitHub; run with ODM_REAL_TOOLS=1"]
async fn installs_the_real_yt_dlp_from_github() {
    if std::env::var("ODM_REAL_TOOLS").as_deref() != Ok("1") {
        eprintln!("skipped: set ODM_REAL_TOOLS=1");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let tools_dir = dir.path().join("tools");
    tools::install(
        &tools_dir,
        &tools::http_client(&odm_engine::config::ProxyConfig::System),
        &ToolSources::default(),
        &|_, _, _| {},
    )
    .await
    .expect("real install");
    let status = tools::status(&EngineConfig {
        tools_dir: Some(tools_dir),
        ytdlp_search: false,
        ffmpeg_search_path: false,
        ..EngineConfig::default()
    });
    let ytdlp = status.ytdlp.expect("the downloaded yt-dlp runs");
    assert_eq!(ytdlp.source, "ODM");
    eprintln!("yt-dlp {}", ytdlp.version);
}
