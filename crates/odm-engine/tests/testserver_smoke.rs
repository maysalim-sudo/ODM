use odm_testserver::{body, Faults, TestServer};

#[tokio::test]
async fn serves_ranges_and_faults() {
    let s = TestServer::start(1000, Faults::default()).await;
    let c = reqwest::Client::new();
    let r = c.get(s.url("/file")).header("Range", "bytes=10-19").send().await.unwrap();
    assert_eq!(r.status(), 206);
    assert_eq!(r.headers()["content-range"], "bytes 10-19/1000");
    assert_eq!(r.bytes().await.unwrap().as_ref(), &body(1000)[10..20]);

    let r = c.get(s.url("/file")).header("Range", "bytes=5000-").send().await.unwrap();
    assert_eq!(r.status(), 416);

    let s = TestServer::start(1000, Faults { no_ranges: true, ..Default::default() }).await;
    let r = c.get(s.url("/file")).header("Range", "bytes=10-19").send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.bytes().await.unwrap().len(), 1000);

    let s = TestServer::start(100_000, Faults { drop_after: Some(1000), drop_times: 1, ..Default::default() }).await;
    let r = c.get(s.url("/file")).send().await.unwrap();
    assert!(r.bytes().await.is_err(), "first request must be cut short");
    let r = c.get(s.url("/file")).send().await.unwrap();
    assert_eq!(r.bytes().await.unwrap().len(), 100_000);
}
