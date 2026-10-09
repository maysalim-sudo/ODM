//! `tools::install` against a local file server. Nothing here touches the real network.
use odm_appcore::tools::{self, ToolSources, CHECKSUM_FAILED, DOWNLOAD_FAILED};
use odm_testserver::files::FileServer;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

fn sha(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in entries {
        w.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

struct Fixture {
    files: Vec<(String, Vec<u8>)>,
}

impl Fixture {
    fn new() -> Fixture {
        let ytdlp = b"fake yt-dlp".to_vec();
        let ffmpeg_zip = zip_of(&[
            (
                "ffmpeg-master-latest-win64-gpl/bin/ffmpeg.exe",
                b"fake ffmpeg",
            ),
            (
                "ffmpeg-master-latest-win64-gpl/bin/ffprobe.exe",
                b"fake ffprobe",
            ),
            (
                "ffmpeg-master-latest-win64-gpl/bin/ffplay.exe",
                b"fake ffplay",
            ),
            ("ffmpeg-master-latest-win64-gpl/LICENSE.txt", b"license"),
        ]);
        let deno_zip = zip_of(&[("deno.exe", b"fake deno")]);
        let files = vec![
            ("yt/yt-dlp.exe".to_string(), ytdlp.clone()),
            (
                "yt/SHA2-256SUMS".into(),
                format!("{}  yt-dlp\n{}  yt-dlp.exe\n", sha(b"other"), sha(&ytdlp)).into_bytes(),
            ),
            ("ff/ffmpeg.zip".into(), ffmpeg_zip.clone()),
            (
                "ff/checksums.sha256".into(),
                format!("{} *ffmpeg.zip\n", sha(&ffmpeg_zip)).into_bytes(),
            ),
            ("deno/deno.zip".into(), deno_zip.clone()),
            (
                "deno/deno.zip.sha256sum".into(),
                format!(
                    "\r\nAlgorithm : SHA256\r\nHash      : {}\r\nPath      : ...deno.zip\r\n\r\n",
                    sha(&deno_zip).to_uppercase()
                )
                .into_bytes(),
            ),
        ];
        Fixture { files }
    }

    fn corrupt(mut self, path: &str) -> Fixture {
        for (p, data) in &mut self.files {
            if p == path {
                data.push(b'!');
            }
        }
        self
    }

    async fn serve(self, cut_off: &[&str]) -> (FileServer, ToolSources) {
        let server =
            FileServer::start(self.files, cut_off.iter().map(|s| s.to_string()).collect()).await;
        let sources = ToolSources {
            ytdlp_url: server.url("yt/yt-dlp.exe"),
            ytdlp_sums_url: server.url("yt/SHA2-256SUMS"),
            ffmpeg_url: server.url("ff/ffmpeg.zip"),
            ffmpeg_sums_url: server.url("ff/checksums.sha256"),
            deno_url: server.url("deno/deno.zip"),
            deno_sums_url: server.url("deno/deno.zip.sha256sum"),
        };
        (server, sources)
    }
}

fn read(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(name)).ok()
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

#[tokio::test]
async fn installs_all_three_tools() {
    let (_server, sources) = Fixture::new().serve(&[]).await;
    let dir = tempfile::tempdir().unwrap();
    let tools = dir.path().join("tools");
    let stages = Mutex::new(Vec::<String>::new());
    let progress = |stage: &str, _done: u64, _total: u64| {
        let mut s = stages.lock().unwrap();
        if s.last().map(String::as_str) != Some(stage) {
            s.push(stage.to_string());
        }
    };
    tools::install(&tools, &reqwest::Client::new(), &sources, &progress)
        .await
        .unwrap();
    assert_eq!(read(&tools, "yt-dlp.exe").as_deref(), Some("fake yt-dlp"));
    assert_eq!(read(&tools, "ffmpeg.exe").as_deref(), Some("fake ffmpeg"));
    assert_eq!(read(&tools, "ffprobe.exe").as_deref(), Some("fake ffprobe"));
    assert_eq!(read(&tools, "deno.exe").as_deref(), Some("fake deno"));
    assert!(!tools.join("ffplay.exe").exists());
    assert!(leftovers(&tools).is_empty(), "{:?}", leftovers(&tools));
    assert_eq!(
        *stages.lock().unwrap(),
        ["yt-dlp", "FFmpeg", "Deno", "Installing"]
    );
}

#[tokio::test]
async fn a_checksum_mismatch_installs_nothing() {
    // The FFmpeg zip changed after its checksum was published.
    let (_server, sources) = Fixture::new().corrupt("ff/ffmpeg.zip").serve(&[]).await;
    let dir = tempfile::tempdir().unwrap();
    let tools = dir.path().join("tools");
    let err = tools::install(&tools, &reqwest::Client::new(), &sources, &|_, _, _| {})
        .await
        .unwrap_err();
    assert_eq!(err, CHECKSUM_FAILED);
    for name in ["yt-dlp.exe", "ffmpeg.exe", "ffprobe.exe", "deno.exe"] {
        assert!(!tools.join(name).exists(), "{name} was installed");
    }
    assert!(leftovers(&tools).is_empty(), "{:?}", leftovers(&tools));
}

#[tokio::test]
async fn a_failed_download_keeps_the_existing_tools() {
    let (_server, sources) = Fixture::new().serve(&["deno/deno.zip"]).await;
    let dir = tempfile::tempdir().unwrap();
    let tools = dir.path().join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::write(tools.join("yt-dlp.exe"), "old yt-dlp").unwrap();
    std::fs::write(tools.join("ffmpeg.exe"), "old ffmpeg").unwrap();
    let err = tools::install(&tools, &reqwest::Client::new(), &sources, &|_, _, _| {})
        .await
        .unwrap_err();
    assert_eq!(err, DOWNLOAD_FAILED);
    assert_eq!(read(&tools, "yt-dlp.exe").as_deref(), Some("old yt-dlp"));
    assert_eq!(read(&tools, "ffmpeg.exe").as_deref(), Some("old ffmpeg"));
    assert!(!tools.join("deno.exe").exists());
    assert!(leftovers(&tools).is_empty(), "{:?}", leftovers(&tools));
}

#[tokio::test]
async fn an_unreachable_server_is_a_download_failure() {
    let (server, sources) = Fixture::new().serve(&[]).await;
    drop(server);
    let dead = ToolSources {
        ytdlp_url: "http://127.0.0.1:1/yt-dlp.exe".into(),
        ..sources
    };
    let dir = tempfile::tempdir().unwrap();
    let err = tools::install(dir.path(), &reqwest::Client::new(), &dead, &|_, _, _| {})
        .await
        .unwrap_err();
    assert_eq!(err, DOWNLOAD_FAILED);
}

#[tokio::test]
async fn a_second_install_replaces_the_tools() {
    let (_server, sources) = Fixture::new().serve(&[]).await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("yt-dlp.exe"), "old yt-dlp").unwrap();
    tools::install(dir.path(), &reqwest::Client::new(), &sources, &|_, _, _| {})
        .await
        .unwrap();
    assert_eq!(
        read(dir.path(), "yt-dlp.exe").as_deref(),
        Some("fake yt-dlp")
    );
}
