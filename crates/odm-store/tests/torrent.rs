use odm_core::DownloadState;
use odm_store::{DownloadRecord, Store, TorrentFileRecord, TorrentMetaRecord};

const HASH: &str = "abcdef0123456789abcdef0123456789abcdef01";

fn torrent(id: &str, hash: &str) -> DownloadRecord {
    DownloadRecord::new_torrent(id, hash, "Pack", "C:/dl/Torrents")
}

fn files() -> Vec<TorrentFileRecord> {
    vec![
        TorrentFileRecord { idx: 0, path: "a.bin".into(), size: 100, selected: true, done: 0 },
        TorrentFileRecord { idx: 2, path: "sub/b.bin".into(), size: 50, selected: true, done: 0 },
    ]
}

async fn version(s: &Store) -> i64 {
    s.call(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .await
        .unwrap()
}

#[tokio::test]
async fn v4_from_fresh() {
    let s = Store::open_in_memory().unwrap();
    assert_eq!(version(&s).await, 4);
    let r = torrent("t", HASH);
    assert_eq!(r.kind, "torrent");
    assert_eq!(r.category, "torrent");
    assert_eq!(r.info_hash.as_deref(), Some(HASH));
    s.insert_download(r.clone()).await.unwrap();
    assert_eq!(s.get_download("t").await.unwrap().unwrap(), r);
}

#[tokio::test]
async fn v4_from_v3() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("odm.db");
    {
        let s = Store::open(&path).unwrap();
        s.insert_download(DownloadRecord::new_file("a", "https://e.com/f", "f", "C:/dl"))
            .await
            .unwrap();
        s.call(|c| {
            c.execute_batch(
                "DROP TABLE torrent_files; DROP TABLE torrent_meta;
                 DROP INDEX idx_downloads_info_hash;
                 ALTER TABLE downloads DROP COLUMN info_hash;",
            )?;
            c.pragma_update(None, "user_version", 3)
        })
        .await
        .unwrap();
    }
    let s = Store::open(&path).unwrap();
    assert_eq!(version(&s).await, 4);
    assert_eq!(s.get_download("a").await.unwrap().unwrap().info_hash, None);
    s.insert_download(torrent("t", HASH)).await.unwrap();
}

#[tokio::test]
async fn info_hash_unique() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(torrent("t1", HASH)).await.unwrap();
    assert!(s.insert_download(torrent("t2", HASH)).await.is_err());
    assert_eq!(s.find_by_info_hash(HASH).await.unwrap().unwrap().id, "t1");
    assert!(s.find_by_info_hash("00").await.unwrap().is_none());
    // Many non-torrent rows have no hash; that is not a clash.
    s.insert_download(DownloadRecord::new_file("f1", "https://e.com/1", "1", "C:/dl")).await.unwrap();
    s.insert_download(DownloadRecord::new_file("f2", "https://e.com/2", "2", "C:/dl")).await.unwrap();
}

#[tokio::test]
async fn meta_round_trips_and_merge_trackers_dedupes() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(torrent("t", HASH)).await.unwrap();
    let m = TorrentMetaRecord {
        download_id: "t".into(),
        source: "magnet:?xt=urn:btih:x".into(),
        trackers: vec!["udp://a:1".into()],
        has_meta: false,
    };
    s.put_torrent_meta(m.clone()).await.unwrap();
    assert_eq!(s.get_torrent_meta("t").await.unwrap(), Some(m));
    let merged = s
        .merge_trackers("t", vec!["udp://b:2".into(), "udp://a:1".into()])
        .await
        .unwrap();
    assert_eq!(merged, vec!["udp://a:1".to_string(), "udp://b:2".to_string()]);
    assert_eq!(s.get_torrent_meta("t").await.unwrap().unwrap().trackers, merged);
}

#[tokio::test]
async fn files_set_meta_and_size() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(torrent("t", HASH)).await.unwrap();
    s.put_torrent_meta(TorrentMetaRecord {
        download_id: "t".into(),
        source: "m".into(),
        trackers: vec![],
        has_meta: false,
    })
    .await
    .unwrap();
    s.replace_torrent_files("t", files()).await.unwrap();
    assert_eq!(s.get_torrent_files("t").await.unwrap(), files());
    assert!(s.get_torrent_meta("t").await.unwrap().unwrap().has_meta);
    assert_eq!(s.get_download("t").await.unwrap().unwrap().size, Some(150));
}

#[tokio::test]
async fn selection_updates_size() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(torrent("t", HASH)).await.unwrap();
    s.replace_torrent_files("t", files()).await.unwrap();
    assert_eq!(s.set_torrent_selection("t", vec![2]).await.unwrap(), 50);
    assert_eq!(s.get_download("t").await.unwrap().unwrap().size, Some(50));
    let sel: Vec<bool> = s.get_torrent_files("t").await.unwrap().iter().map(|f| f.selected).collect();
    assert_eq!(sel, vec![false, true]);
}

#[tokio::test]
async fn progress_counts_selected_files_in_bytes() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(torrent("t", HASH)).await.unwrap();
    s.replace_torrent_files("t", files()).await.unwrap();
    s.save_torrent_progress("t", vec![(0, 40), (2, 50)]).await.unwrap();
    assert_eq!(s.get_with_bytes("t").await.unwrap().unwrap().1, 90);
    s.set_torrent_selection("t", vec![0]).await.unwrap();
    assert_eq!(s.get_with_bytes("t").await.unwrap().unwrap().1, 40);
    let listed = s.list_with_bytes().await.unwrap();
    assert_eq!(listed[0].1, 40);
}

#[tokio::test]
async fn cascade_deletes_torrent_rows() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(torrent("t", HASH)).await.unwrap();
    s.put_torrent_meta(TorrentMetaRecord {
        download_id: "t".into(),
        source: "m".into(),
        trackers: vec![],
        has_meta: false,
    })
    .await
    .unwrap();
    s.replace_torrent_files("t", files()).await.unwrap();
    s.delete_download("t").await.unwrap();
    assert!(s.get_torrent_meta("t").await.unwrap().is_none());
    assert!(s.get_torrent_files("t").await.unwrap().is_empty());
    // The hash is free again.
    s.insert_download(torrent("t2", HASH)).await.unwrap();
}

#[tokio::test]
async fn completed_torrent_can_be_requeued() {
    let s = Store::open_in_memory().unwrap();
    s.insert_download(torrent("t", HASH)).await.unwrap();
    for st in [DownloadState::Probing, DownloadState::Downloading, DownloadState::Completed, DownloadState::Queued] {
        s.set_state("t", st, None).await.unwrap();
    }
    assert_eq!(s.get_download("t").await.unwrap().unwrap().state, DownloadState::Queued);
}
