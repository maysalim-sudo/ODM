use odm_core::ErrorKind;
use odm_engine::chunks::ChunkTable;
use odm_engine::worker::{run_chunk, ChunkCtx};
use odm_engine::SpeedLimiter;
use odm_store::ChunkRecord;
use odm_testserver::{body, Faults, TestServer};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

fn ctx(url: String, file: std::fs::File, require_partial: bool, validator: Option<&str>) -> ChunkCtx {
    ChunkCtx {
        client: reqwest::Client::new(),
        url,
        headers: Default::default(),
        validator: validator.map(str::to_string),
        file: Arc::new(file),
        global_limit: Arc::new(SpeedLimiter::new(0)),
        local_limit: None,
        cancel: CancellationToken::new(),
        require_partial,
        session_bytes: Arc::new(AtomicU64::new(0)),
    }
}

fn tmpfile(len: u64) -> (tempfile::TempDir, std::path::PathBuf, std::fs::File) {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("out.bin");
    let f = std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(&p).unwrap();
    f.set_len(len).unwrap();
    (d, p, f)
}

fn table(start: u64, end: Option<u64>, written: u64) -> ChunkTable {
    ChunkTable::from_records(&[ChunkRecord { idx: 0, start, end, written }])
}

#[tokio::test]
async fn downloads_a_range_exactly() {
    let s = TestServer::start(300_000, Faults::default()).await;
    let (_d, p, f) = tmpfile(300_000);
    let t = table(100_000, Some(199_999), 0);
    run_chunk(&ctx(s.url("/file"), f, true, Some("\"v1\"")), &t.refs()[0]).await.unwrap();
    let got = std::fs::read(&p).unwrap();
    assert_eq!(&got[100_000..200_000], &body(300_000)[100_000..200_000]);
    assert!(got[..100_000].iter().all(|&b| b == 0));
    assert_eq!(t.total_written(), 100_000);
}

#[tokio::test]
async fn stops_at_end_even_if_server_sends_more() {
    let s = TestServer::start(50_000, Faults { no_ranges: true, ..Default::default() }).await;
    let (_d, p, f) = tmpfile(1_000);
    let t = table(0, Some(999), 0);
    run_chunk(&ctx(s.url("/file"), f, false, None), &t.refs()[0]).await.unwrap();
    let got = std::fs::read(&p).unwrap();
    assert_eq!(got.len(), 1_000);
    assert_eq!(got, &body(50_000)[..1_000]);
}

#[tokio::test]
async fn range_ignored_when_partial_required() {
    let s = TestServer::start(50_000, Faults { no_ranges: true, ..Default::default() }).await;
    let (_d, _p, f) = tmpfile(50_000);
    let t = table(10_000, Some(19_999), 0);
    let e = run_chunk(&ctx(s.url("/file"), f, true, None), &t.refs()[0]).await.unwrap_err();
    assert_eq!(e.error.kind, ErrorKind::RangeIgnored);
    assert_eq!(t.total_written(), 0);
}

#[tokio::test]
async fn wrong_content_range_start_is_range_ignored() {
    let s = TestServer::start(50_000, Faults { wrong_range_start: true, ..Default::default() }).await;
    let (_d, _p, f) = tmpfile(50_000);
    let t = table(10_000, Some(19_999), 0);
    let e = run_chunk(&ctx(s.url("/file"), f, true, None), &t.refs()[0]).await.unwrap_err();
    assert_eq!(e.error.kind, ErrorKind::RangeIgnored);
}

#[tokio::test]
async fn dropped_connection_then_resume_completes() {
    let s = TestServer::start(200_000, Faults { drop_after: Some(50_000), drop_times: 1, ..Default::default() }).await;
    let (_d, p, f) = tmpfile(200_000);
    let t = table(0, Some(199_999), 0);
    let c = ctx(s.url("/file"), f, true, Some("\"v1\""));
    let e = run_chunk(&c, &t.refs()[0]).await.unwrap_err();
    assert!(e.error.kind.is_transient(), "{:?}", e.error);
    let partial = t.total_written();
    assert!(partial > 0 && partial <= 50_000, "{partial}");
    run_chunk(&c, &t.refs()[0]).await.unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), body(200_000));
}

#[tokio::test]
async fn changed_file_is_detected_via_if_range() {
    let s = TestServer::start(50_000, Faults { etag_changes_after: Some(0), ..Default::default() }).await;
    let (_d, _p, f) = tmpfile(50_000);
    let t = table(0, Some(49_999), 10_000);
    let e = run_chunk(&ctx(s.url("/file"), f, true, Some("\"v1\"")), &t.refs()[0]).await.unwrap_err();
    assert_eq!(e.error.kind, ErrorKind::FileChanged);
}

#[tokio::test]
async fn cancel_stops_promptly() {
    let s = TestServer::start(5_000_000, Faults { bytes_per_sec: Some(200_000), ..Default::default() }).await;
    let (_d, _p, f) = tmpfile(5_000_000);
    let t = table(0, Some(4_999_999), 0);
    let c = ctx(s.url("/file"), f, true, None);
    let tok = c.cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        tok.cancel();
    });
    let started = std::time::Instant::now();
    let e = run_chunk(&c, &t.refs()[0]).await.unwrap_err();
    assert_eq!(e.error.kind, ErrorKind::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(t.total_written() < 5_000_000);
}

#[tokio::test]
async fn statuses_map_to_kinds_with_retry_after() {
    let s = TestServer::start(1000, Faults { fail_status: Some(429), fail_times: 9, ..Default::default() }).await;
    let (_d, _p, f) = tmpfile(1000);
    let t = table(0, Some(999), 0);
    let e = run_chunk(&ctx(s.url("/file"), f, true, None), &t.refs()[0]).await.unwrap_err();
    assert_eq!(e.error.kind, ErrorKind::HttpTransient(429));
    assert_eq!(e.retry_after, Some(Duration::from_secs(1)));

    let s = TestServer::start(1000, Faults { forbid_after: Some(0), ..Default::default() }).await;
    let (_d, _p, f) = tmpfile(1000);
    let t = table(0, Some(999), 0);
    let e = run_chunk(&ctx(s.url("/file"), f, true, None), &t.refs()[0]).await.unwrap_err();
    assert_eq!(e.error.kind, ErrorKind::LinkExpired);
}

#[tokio::test]
async fn open_ended_stream_completes_and_sets_end() {
    let s = TestServer::start(70_000, Faults { no_ranges: true, no_length: true, ..Default::default() }).await;
    let (_d, p, f) = tmpfile(0);
    let t = table(0, None, 0);
    run_chunk(&ctx(s.url("/file"), f, false, None), &t.refs()[0]).await.unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), body(70_000));
    assert_eq!(t.snapshot()[0].end, Some(69_999));
}
