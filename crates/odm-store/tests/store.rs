use odm_core::DownloadState;
use odm_store::{ChunkRecord, DownloadRecord, HlsMeta, ProbeMeta, SegmentRecord, Store};

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
        assert_eq!(v, 4);
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
            assert_eq!(supported, 4);
        }
        Err(e) => panic!("wrong error: {e}"),
        Ok(_) => panic!("opened a newer schema"),
    }
}

#[tokio::test]
async fn v1_database_is_migrated_to_the_latest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("odm.db");
    {
        let s = Store::open(&path).unwrap();
        s.insert_download(rec("a")).await.unwrap();
        // Rewind to a V1-shaped database: drop what V2 to V4 added and the version.
        s.call(|c| {
            c.execute_batch(
                "DROP TABLE torrent_files; DROP TABLE torrent_meta;
                 DROP INDEX idx_downloads_info_hash;
                 ALTER TABLE downloads DROP COLUMN info_hash;
                 ALTER TABLE downloads DROP COLUMN auto_place;
                 ALTER TABLE downloads DROP COLUMN pause_reason;",
            )?;
            c.pragma_update(None, "user_version", 1)
        })
        .await
        .unwrap();
    }
    let s = Store::open(&path).unwrap();
    let got = s.get_download("a").await.unwrap().unwrap();
    assert!(!got.auto_place);
    assert_eq!(got.pause_reason, None);
    let v: i64 = s
        .call(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .await
        .unwrap();
    assert_eq!(v, 4);
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

#[tokio::test]
async fn count_by_state_groups_rows() {
    let s = Store::open_in_memory().unwrap();
    for (id, st) in [
        ("a", DownloadState::Queued),
        ("b", DownloadState::Queued),
        ("c", DownloadState::Paused),
        ("d", DownloadState::Completed),
    ] {
        let mut r = rec(id);
        r.state = st;
        s.insert_download(r).await.unwrap();
    }
    let counts: std::collections::HashMap<DownloadState, u64> =
        s.count_by_state().await.unwrap().into_iter().collect();
    assert_eq!(
        counts,
        std::collections::HashMap::from([
            (DownloadState::Queued, 2),
            (DownloadState::Paused, 1),
            (DownloadState::Completed, 1),
        ])
    );
}

#[tokio::test]
async fn set_url_clears_final_url_keeps_validator() {
    let s = Store::open_in_memory().unwrap();
    let mut r = rec("a");
    r.final_url = Some("https://cdn.e.com/old".into());
    r.etag = Some("\"abc\"".into());
    r.last_modified = Some("Mon, 01 Jan 2026 00:00:00 GMT".into());
    s.insert_download(r.clone()).await.unwrap();
    s.set_url("a", "https://e.com/new?token=2".into())
        .await
        .unwrap();
    let got = s.get_download("a").await.unwrap().unwrap();
    assert_eq!(got.url, "https://e.com/new?token=2");
    assert_eq!(got.final_url, None);
    assert_eq!(got.etag, r.etag);
    assert_eq!(got.last_modified, r.last_modified);
}

#[tokio::test]
async fn segments_round_trip_and_bytes() {
    let s = Store::open_in_memory().unwrap();
    let r = DownloadRecord::new_hls("h", "https://e.com/m.m3u8", "v.mp4", "C:/dl");
    assert_eq!(r.kind, "hls");
    s.insert_download(r).await.unwrap();
    let seg = |idx: u32| SegmentRecord {
        idx,
        url: format!("https://e.com/s{idx}.ts"),
        duration_ms: 4000,
        state: "queued".into(),
        bytes: 0,
    };
    s.replace_segments("h", vec![seg(1), seg(2), seg(3)])
        .await
        .unwrap();
    s.mark_segment_done("h", 2, 500).await.unwrap();
    let got = s.get_segments("h").await.unwrap();
    let states: Vec<&str> = got.iter().map(|x| x.state.as_str()).collect();
    assert_eq!(states, ["queued", "done", "queued"]);
    assert_eq!(got[1].bytes, 500);
    assert_eq!(got[2].url, "https://e.com/s3.ts");
    assert_eq!(s.get_with_bytes("h").await.unwrap().unwrap().1, 500);
    assert_eq!(s.list_with_bytes().await.unwrap()[0].1, 500);

    // Replacing discards the old rows.
    s.replace_segments("h", vec![seg(1)]).await.unwrap();
    assert_eq!(s.get_segments("h").await.unwrap().len(), 1);
    assert_eq!(s.get_with_bytes("h").await.unwrap().unwrap().1, 0);
}

#[tokio::test]
async fn hls_meta_upsert() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(DownloadRecord::new_hls("h", "u", "v.mp4", "C:/dl"))
        .await
        .unwrap();
    assert!(s.get_hls_meta("h").await.unwrap().is_none());
    let m = |v: &str| HlsMeta {
        variant_url: v.into(),
        key_url: Some("k".into()),
        iv: None,
        map_url: Some("m".into()),
    };
    s.set_hls_meta("h", m("first")).await.unwrap();
    s.set_hls_meta("h", m("second")).await.unwrap();
    assert_eq!(s.get_hls_meta("h").await.unwrap().unwrap(), m("second"));
}

#[tokio::test]
async fn set_target_updates_and_locks() {
    let s = Store::open_in_memory().unwrap();
    let mut r = rec("a");
    r.auto_place = true;
    s.insert_download(r).await.unwrap();

    s.set_target("a", Some("new.zip".into()), None)
        .await
        .unwrap();
    let d = s.get_download("a").await.unwrap().unwrap();
    assert_eq!(
        (d.filename.as_str(), d.save_dir.as_str()),
        ("new.zip", "C:/dl")
    );
    assert!(d.filename_locked && d.auto_place);

    s.set_target("a", None, Some("D:/else".into()))
        .await
        .unwrap();
    let d = s.get_download("a").await.unwrap().unwrap();
    assert_eq!(
        (d.filename.as_str(), d.save_dir.as_str()),
        ("new.zip", "D:/else")
    );
    assert!(d.filename_locked && !d.auto_place);
}

#[tokio::test]
async fn pause_reason_is_set_by_set_paused_and_cleared_by_any_other_state() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    assert_eq!(
        s.get_download("a").await.unwrap().unwrap().pause_reason,
        None
    );
    s.set_paused("a", Some("schedule".into())).await.unwrap();
    let r = s.get_download("a").await.unwrap().unwrap();
    assert_eq!(r.state, DownloadState::Paused);
    assert_eq!(r.pause_reason.as_deref(), Some("schedule"));
    s.set_state("a", DownloadState::Queued, None).await.unwrap();
    assert_eq!(
        s.get_download("a").await.unwrap().unwrap().pause_reason,
        None
    );
    s.set_paused("a", Some("exists".into())).await.unwrap();
    // A plain pause (by the user) has no reason.
    s.set_state("a", DownloadState::Paused, None).await.unwrap();
    assert_eq!(
        s.get_download("a").await.unwrap().unwrap().pause_reason,
        None
    );
}

#[tokio::test]
async fn delete_completed_before_removes_only_old_completed_records() {
    let s = Store::open_in_memory().unwrap();
    for id in ["old", "new", "failed", "queued"] {
        s.insert_download(rec(id)).await.unwrap();
    }
    s.set_state("old", DownloadState::Completed, None)
        .await
        .unwrap();
    s.set_state("new", DownloadState::Completed, None)
        .await
        .unwrap();
    s.set_state("failed", DownloadState::Failed, None)
        .await
        .unwrap();
    s.call(|c| {
        c.execute(
            "UPDATE downloads SET finished_at = 1000 WHERE id IN ('old','failed')",
            [],
        )
    })
    .await
    .unwrap();
    s.call(|c| {
        c.execute(
            "UPDATE downloads SET finished_at = 5000 WHERE id = 'new'",
            [],
        )
    })
    .await
    .unwrap();
    let gone = s.delete_completed_before(2000).await.unwrap();
    assert_eq!(gone, vec!["old".to_string()]);
    let left: Vec<String> = s
        .list_downloads()
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(left.len(), 3);
    assert!(!left.contains(&"old".to_string()));
}

#[tokio::test]
async fn set_speed_limit_round_trips() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(rec("a")).await.unwrap();
    s.set_speed_limit("a", Some(65536)).await.unwrap();
    assert_eq!(
        s.get_download("a").await.unwrap().unwrap().speed_limit,
        Some(65536)
    );
    s.set_speed_limit("a", None).await.unwrap();
    assert_eq!(
        s.get_download("a").await.unwrap().unwrap().speed_limit,
        None
    );
}
