//! Downloads the real video tools from GitHub (about 200 MB) into a temp folder and checks that
//! each one runs. Opt-in: `ODM_REAL_TOOLS=1 cargo test -p odm-appcore --test tools_real -- --ignored`.
use odm_appcore::tools::{self, ToolSources};
use odm_engine::EngineConfig;

#[tokio::test]
#[ignore = "downloads ~200 MB from GitHub; run with ODM_REAL_TOOLS=1"]
async fn installs_the_real_tools_from_github() {
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
    for exe in ["yt-dlp.exe", "ffmpeg.exe", "ffprobe.exe", "deno.exe"] {
        assert!(tools_dir.join(exe).is_file(), "{exe} missing");
    }
    // Only ODM's folder is searched, so every tool found here is the one just installed.
    let cfg = EngineConfig {
        tools_dir: Some(tools_dir.clone()),
        ytdlp_search: false,
        ffmpeg_search_path: false,
        ..EngineConfig::default()
    };
    let s = tools::status(&cfg);
    let ff = s.ffmpeg.expect("ffmpeg runs");
    let yt = s.ytdlp.expect("yt-dlp runs");
    let deno = s.deno.expect("deno runs");
    eprintln!(
        "ffmpeg {} ({}) · yt-dlp {} ({}) · deno {}",
        ff.version, ff.source, yt.version, yt.source, deno.version
    );
    assert_eq!(ff.source, "ODM");
    assert_eq!(yt.source, "ODM");
}
