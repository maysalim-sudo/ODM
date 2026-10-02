use odm_core::DownloadState;
use odm_store::{ChunkRecord, DownloadRecord, ProbeMeta, Store};

fn rec(id: &str) -> DownloadRecord {
    DownloadRecord::new_file(id, "https://e.com/f.bin", "f.bin", "C:/dl")
}

#[tokio::test]
async fn fresh_db_is_migrated_and_reopen_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("odm.db");
    {
        let s = Store::open(&path).unwrap();
        let v: i64 = s
            .call(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
            .await
            .unwrap();
        assert_eq!(v, 2);
        s.insert_download(rec("a")).await.unwrap();
    }
    let s = Store::open(&path).unwrap();
    assert!(s.get_download("a").await.unwrap().is_some());
}

#[tokio::test]
async fn insert_get_roundtrip() {
    let s = Store::open_in_memory().unwrap();
    let mut r = rec("a");
    r.size = Some(123);
    r.page_title = Some("T".into());
    s.insert_download(r.clone()).await.unwrap();
    assert_eq!(s.get_download("a").await.unwrap().unwrap(), r);
    assert!(s.get_download("nope").await.unwrap().is_none());
}

#[tokio::test]
async fn set_state_tracks_timestamps_and_errors() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    s.set_state("a", DownloadState::Downloading, None)
        .await
        .unwrap();
    let d = s.get_download("a").await.unwrap().unwrap();
    assert_eq!(d.state, DownloadState::Downloading);
    assert!(d.started_at.is_some() && d.finished_at.is_none());

    s.set_state(
        "a",
        DownloadState::Failed,
        Some(("http_404".into(), "gone".into())),
    )
    .await
    .unwrap();
    let d = s.get_download("a").await.unwrap().unwrap();
    assert_eq!(d.error_kind.as_deref(), Some("http_404"));
    assert!(d.finished_at.is_some());

    s.set_state("a", DownloadState::Queued, None).await.unwrap();
    let d = s.get_download("a").await.unwrap().unwrap();
    assert!(d.error_kind.is_none() && d.error_msg.is_none() && d.finished_at.is_none());
}

#[tokio::test]
async fn probe_respects_filename_lock() {
    let s = Store::open_in_memory().unwrap();
    let mut locked = rec("a");
    locked.filename_locked = true;
    s.insert_download(locked).await.unwrap();
    s.insert_download(rec("b")).await.unwrap();
    let meta = ProbeMeta {
        size: Some(10),
        filename: Some("server.zip".into()),
        ..Default::default()
    };
    s.update_probe("a", meta.clone()).await.unwrap();
    s.update_probe("b", meta).await.unwrap();
    assert_eq!(
        s.get_download("a").await.unwrap().unwrap().filename,
        "f.bin"
    );
    let b = s.get_download("b").await.unwrap().unwrap();
    assert_eq!((b.filename.as_str(), b.size), ("server.zip", Some(10)));
}

#[tokio::test]
async fn headers_replace() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    s.set_headers(
        "a",
        vec![
            ("Cookie".into(), "x=1".into()),
            ("Referer".into(), "r".into()),
        ],
    )
    .await
    .unwrap();
    s.set_headers("a", vec![("Cookie".into(), "x=2".into())])
        .await
        .unwrap();
    assert_eq!(
        s.get_headers("a").await.unwrap(),
        vec![("Cookie".to_string(), "x=2".to_string())]
    );
}

#[tokio::test]
async fn chunks_replace_and_progress() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    let chunks = vec![
        ChunkRecord {
            idx: 0,
            start: 0,
            end: Some(99),
            written: 0,
        },
        ChunkRecord {
            idx: 1,
            start: 100,
            end: None,
            written: 0,
        },
    ];
    s.replace_chunks("a", chunks).await.unwrap();
    s.save_progress("a", vec![(0, 50), (1, 7)]).await.unwrap();
    let got = s.get_chunks("a").await.unwrap();
    assert_eq!(
        got[0],
        ChunkRecord {
            idx: 0,
            start: 0,
            end: Some(99),
            written: 50
        }
    );
    assert_eq!(
        got[1],
        ChunkRecord {
            idx: 1,
            start: 100,
            end: None,
            written: 7
        }
    );
    s.replace_chunks(
        "a",
        vec![ChunkRecord {
            idx: 0,
            start: 0,
            end: Some(9),
            written: 0,
        }],
    )
    .await
    .unwrap();
    assert_eq!(s.get_chunks("a").await.unwrap().len(), 1);
}

#[tokio::test]
async fn delete_cascades_and_reset_interrupted() {
    let s = Store::open_in_memory().unwrap();
    for id in ["a", "b", "c"] {
        s.insert_download(rec(id)).await.unwrap();
    }
    s.set_headers("a", vec![("A".into(), "1".into())])
        .await
        .unwrap();
    s.replace_chunks(
        "a",
        vec![ChunkRecord {
            idx: 0,
            start: 0,
            end: None,
            written: 0,
        }],
    )
    .await
    .unwrap();
    s.delete_download("a").await.unwrap();
    let n: i64 = s
        .call(|c| c.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0)))
        .await
        .unwrap();
    assert_eq!(n, 0);

    s.set_state("b", DownloadState::Downloading, None)
        .await
        .unwrap();
    s.set_state("c", DownloadState::Paused, None).await.unwrap();
    assert_eq!(s.reset_interrupted().await.unwrap(), 1);
    assert_eq!(
        s.get_download("b").await.unwrap().unwrap().state,
        DownloadState::Queued
    );
    assert_eq!(
        s.get_download("c").await.unwrap().unwrap().state,
        DownloadState::Paused
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_callers_are_serialized() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    let chunks = (0..16u32)
        .map(|i| ChunkRecord {
            idx: i,
            start: i as u64 * 10,
            end: Some(i as u64 * 10 + 9),
            written: 0,
        })
        .collect();
    s.replace_chunks("a", chunks).await.unwrap();
    let mut tasks = Vec::new();
    for i in 0..16u32 {
        let s = s.clone();
        tasks.push(tokio::spawn(async move {
            for w in 1..=10u64 {
                s.save_progress("a", vec![(i, w)]).await.unwrap();
                s.get_chunks("a").await.unwrap();
            }
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    assert!(s
        .get_chunks("a")
        .await
        .unwrap()
        .iter()
        .all(|c| c.written == 10));
}

#[tokio::test]
async fn newer_schema_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("odm.db");
    {
        let s = Store::open(&path).unwrap();
        s.call(|c| c.pragma_update(None, "user_version", 99))
            .await
            .unwrap();
    }
    match Store::open(&path) {
        Err(odm_store::StoreError::NewerSchema { found, supported }) => {
            assert_eq!(found, 99);
            assert_eq!(supported, 2);
        }
        Err(e) => panic!("wrong error: {e}"),
        Ok(_) => panic!("opened a newer schema"),
    }
}

#[tokio::test]
async fn v1_database_is_migrated_to_v2() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("odm.db");
    {
        let s = Store::open(&path).unwrap();
        s.insert_download(rec("a")).await.unwrap();
        // Rewind to a V1-shaped database: drop the V2 column and the version.
        s.call(|c| {
            c.execute_batch("ALTER TABLE downloads DROP COLUMN auto_place;")?;
            c.pragma_update(None, "user_version", 1)
        })
        .await
        .unwrap();
    }
    let s = Store::open(&path).unwrap();
    let got = s.get_download("a").await.unwrap().unwrap();
    assert!(!got.auto_place);
    let v: i64 = s
        .call(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .await
        .unwrap();
    assert_eq!(v, 2);
}

#[tokio::test]
async fn insert_with_headers_is_atomic() {
    let s = Store::open_in_memory().unwrap();
    let mut r = rec("a");
    r.auto_place = true;
    s.insert_download_with_headers(r, vec![("Cookie".into(), "x=1".into())])
        .await
        .unwrap();
    assert!(s.get_download("a").await.unwrap().unwrap().auto_place);
    assert_eq!(s.get_headers("a").await.unwrap().len(), 1);

    // Second insert with the same id fails as a whole: no extra header rows appear.
    let err = s
        .insert_download_with_headers(rec("a"), vec![("Referer".into(), "r".into())])
        .await;
    assert!(err.is_err());
    assert_eq!(s.get_headers("a").await.unwrap().len(), 1);
}

#[tokio::test]
async fn bytes_come_from_chunks() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    s.insert_download(rec("b")).await.unwrap();
    s.replace_chunks(
        "a",
        vec![
            ChunkRecord {
                idx: 0,
                start: 0,
                end: Some(99),
                written: 40,
            },
            ChunkRecord {
                idx: 1,
                start: 100,
                end: Some(199),
                written: 2,
            },
        ],
    )
    .await
    .unwrap();
    let (ra, bytes_a) = s.get_with_bytes("a").await.unwrap().unwrap();
    assert_eq!((ra.id.as_str(), bytes_a), ("a", 42));
    assert_eq!(s.get_with_bytes("b").await.unwrap().unwrap().1, 0);
    assert!(s.get_with_bytes("nope").await.unwrap().is_none());
    let all = s.list_with_bytes().await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all.iter().find(|(r, _)| r.id == "a").unwrap().1, 42);
}

#[tokio::test]
async fn set_placement_updates_category_and_dir() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    s.set_placement("a", "video".into(), "C:/dl/Videos".into())
        .await
        .unwrap();
    let r = s.get_download("a").await.unwrap().unwrap();
    assert_eq!(
        (r.category.as_str(), r.save_dir.as_str()),
        ("video", "C:/dl/Videos")
    );
}
