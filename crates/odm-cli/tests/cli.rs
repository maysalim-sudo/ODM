use odm_cli::{run_get, GetArgs};
use odm_testserver::{body, Faults, TestServer};

#[tokio::test(flavor = "multi_thread")]
async fn get_downloads_to_dir() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(3 << 20, Faults::default()).await;
    let path = run_get(GetArgs {
        url: s.url("/file"),
        dir: Some(d.path().to_path_buf()),
        name: Some("out.bin".into()),
        connections: Some(4),
        limit_bps: 0,
        db: Some(d.path().join("cli.db")),
        quiet: true,
    })
    .await
    .unwrap();
    assert_eq!(path, d.path().join("out.bin"));
    assert_eq!(std::fs::read(path).unwrap(), body(3 << 20));
}

#[tokio::test(flavor = "multi_thread")]
async fn get_reports_failure() {
    let d = tempfile::tempdir().unwrap();
    let s = TestServer::start(10, Faults { fail_status: Some(404), fail_times: 99, ..Default::default() }).await;
    let err = run_get(GetArgs {
        url: s.url("/file"),
        dir: Some(d.path().to_path_buf()),
        name: None,
        connections: None,
        limit_bps: 0,
        db: Some(d.path().join("cli.db")),
        quiet: true,
    })
    .await
    .unwrap_err();
    assert!(err.contains("http_404"), "{err}");
}
