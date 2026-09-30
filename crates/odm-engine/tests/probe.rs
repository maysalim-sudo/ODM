use odm_core::ErrorKind;
use odm_engine::probe::{parse_content_range_total, probe};
use odm_testserver::{Faults, TestServer};
use reqwest::header::HeaderMap;

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

#[test]
fn content_range_parsing() {
    assert_eq!(parse_content_range_total("bytes 0-0/1234"), Some(1234));
    assert_eq!(parse_content_range_total("bytes */0"), Some(0));
    assert_eq!(parse_content_range_total("bytes 0-0/*"), None);
    assert_eq!(parse_content_range_total("garbage"), None);
}

#[tokio::test]
async fn probe_range_server() {
    let s = TestServer::start(5_000_000, Faults::default()).await;
    let p = probe(&client(), &s.url("/file"), &HeaderMap::new())
        .await
        .unwrap();
    assert_eq!(p.size, Some(5_000_000));
    assert!(p.supports_range);
    assert_eq!(p.etag.as_deref(), Some("\"v1\""));
    assert_eq!(p.validator().as_deref(), Some("\"v1\""));
    assert_eq!(p.mime.as_deref(), Some("application/octet-stream"));
}

#[tokio::test]
async fn probe_no_range_server() {
    let s = TestServer::start(
        1234,
        Faults {
            no_ranges: true,
            ..Default::default()
        },
    )
    .await;
    let p = probe(&client(), &s.url("/file"), &HeaderMap::new())
        .await
        .unwrap();
    assert_eq!((p.size, p.supports_range), (Some(1234), false));
}

#[tokio::test]
async fn probe_unknown_size() {
    let s = TestServer::start(
        1234,
        Faults {
            no_ranges: true,
            no_length: true,
            ..Default::default()
        },
    )
    .await;
    let p = probe(&client(), &s.url("/file"), &HeaderMap::new())
        .await
        .unwrap();
    assert_eq!((p.size, p.supports_range), (None, false));
}

#[tokio::test]
async fn probe_zero_length_416() {
    let s = TestServer::start(0, Faults::default()).await;
    let p = probe(&client(), &s.url("/file"), &HeaderMap::new())
        .await
        .unwrap();
    assert_eq!(p.size, Some(0));
}

#[tokio::test]
async fn probe_zero_length_200() {
    let s = TestServer::start(0, Faults::default()).await;
    let p = probe(&client(), &s.url("/empty"), &HeaderMap::new())
        .await
        .unwrap();
    assert_eq!(p.size, Some(0));
}

#[tokio::test]
async fn probe_follows_redirect_and_reads_disposition() {
    let s = TestServer::start(
        100,
        Faults {
            filename: Some("report.pdf".into()),
            ..Default::default()
        },
    )
    .await;
    let p = probe(&client(), &s.url("/redirect"), &HeaderMap::new())
        .await
        .unwrap();
    assert!(p.final_url.ends_with("/file"), "{}", p.final_url);
    assert_eq!(p.filename.as_deref(), Some("report.pdf"));
}

#[tokio::test]
async fn probe_http_error_is_classified() {
    let s = TestServer::start(
        100,
        Faults {
            fail_status: Some(404),
            fail_times: 99,
            ..Default::default()
        },
    )
    .await;
    let e = probe(&client(), &s.url("/file"), &HeaderMap::new())
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Http(404));
    let s = TestServer::start(
        100,
        Faults {
            fail_status: Some(503),
            fail_times: 99,
            ..Default::default()
        },
    )
    .await;
    let e = probe(&client(), &s.url("/file"), &HeaderMap::new())
        .await
        .unwrap_err();
    assert!(e.kind.is_transient());
}
