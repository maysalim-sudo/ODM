use odm_testserver::{body, Faults, TestServer};

#[tokio::test]
async fn serves_ranges_and_faults() {
    let s = TestServer::start(1000, Faults::default()).await;
    let c = reqwest::Client::new();
    let r = c
        .get(s.url("/file"))
        .header("Range", "bytes=10-19")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 206);
    assert_eq!(r.headers()["content-range"], "bytes 10-19/1000");
    assert_eq!(r.bytes().await.unwrap().as_ref(), &body(1000)[10..20]);

    let r = c
        .get(s.url("/file"))
        .header("Range", "bytes=5000-")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 416);

    let s = TestServer::start(
        1000,
        Faults {
            no_ranges: true,
            ..Default::default()
        },
    )
    .await;
    let r = c
        .get(s.url("/file"))
        .header("Range", "bytes=10-19")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.bytes().await.unwrap().len(), 1000);

    let s = TestServer::start(
        100_000,
        Faults {
            drop_after: Some(1000),
            drop_times: 1,
            ..Default::default()
        },
    )
    .await;
    let r = c.get(s.url("/file")).send().await.unwrap();
    assert!(r.bytes().await.is_err(), "first request must be cut short");
    let r = c.get(s.url("/file")).send().await.unwrap();
    assert_eq!(r.bytes().await.unwrap().len(), 100_000);
}

#[tokio::test]
async fn sequential_requests_peak_at_one_in_flight() {
    let s = TestServer::start(50_000, Faults::default()).await;
    let c = reqwest::Client::new();
    for _ in 0..5 {
        let r = c.get(s.url("/file")).send().await.unwrap();
        assert_eq!(r.bytes().await.unwrap().len(), 50_000);
    }
    assert_eq!(s.max_concurrent(), 1);
}

#[tokio::test]
async fn concurrent_throttled_requests_overlap() {
    let s = TestServer::start(
        200_000,
        Faults {
            bytes_per_sec: Some(200_000),
            ..Default::default()
        },
    )
    .await;
    let c = reqwest::Client::new();
    let mut tasks = Vec::new();
    for _ in 0..3 {
        let c = c.clone();
        let u = s.url("/file");
        tasks.push(tokio::spawn(async move {
            c.get(u).send().await.unwrap().bytes().await.unwrap().len()
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), 200_000);
    }
    assert!(s.max_concurrent() >= 2, "got {}", s.max_concurrent());
}

#[tokio::test]
async fn max_parallel_refuses_extra_requests_but_not_the_probe() {
    let s = TestServer::start(
        200_000,
        Faults {
            max_parallel: Some(1),
            bytes_per_sec: Some(100_000),
            ..Default::default()
        },
    )
    .await;
    let c = reqwest::Client::new();
    // Hold one body in flight: headers arrive, the body is still streaming.
    let first = c.get(s.url("/file")).send().await.unwrap();
    assert_eq!(first.status(), 200);

    let second = c.get(s.url("/file")).send().await.unwrap();
    assert_eq!(second.status(), 429);
    assert_eq!(second.headers()["retry-after"], "1");
    assert!(second.bytes().await.unwrap().is_empty());

    let probe = c
        .get(s.url("/file"))
        .header("Range", "bytes=0-0")
        .send()
        .await
        .unwrap();
    assert_eq!(probe.status(), 206);

    assert_eq!(first.bytes().await.unwrap().len(), 200_000);
    assert!(s.max_concurrent() <= 2, "got {}", s.max_concurrent());
}

#[tokio::test]
async fn hls_server_serves_synthetic_playlist() {
    use odm_engine::hls::crypto::{decrypt_aes128, iv_for};
    use odm_engine::hls::playlist::{Key, KeyMethod};
    use odm_testserver::hls::{write_synthetic, HlsFaults, HlsServer};

    let dir = tempfile::tempdir().unwrap();
    let plain = write_synthetic(dir.path(), 3, 1000, true);
    assert_eq!(plain.len(), 3);
    assert_eq!(plain[1][0], 62);
    let s = HlsServer::start(dir.path().to_path_buf(), HlsFaults::default()).await;
    let c = reqwest::Client::new();

    let r = c.get(s.url("/media.m3u8")).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["content-type"], "application/vnd.apple.mpegurl");
    assert!(r
        .text()
        .await
        .unwrap()
        .contains("#EXT-X-KEY:METHOD=AES-128"));

    let enc = c
        .get(s.url("/s1.ts"))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(
        s.segment_requests.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    let key_bytes = c
        .get(s.url("/key.bin"))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let key: [u8; 16] = key_bytes.as_ref().try_into().unwrap();
    let k = Key {
        method: KeyMethod::Aes128,
        uri: None,
        iv: None,
    };
    let dec = decrypt_aes128(&key, &iv_for(&k, 0), &enc).unwrap();
    assert_eq!(dec, plain[0]);

    assert_eq!(c.get(s.url("/nope")).send().await.unwrap().status(), 404);
}
