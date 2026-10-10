use odm_core::DownloadState;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::models::{now_ms, ChunkRecord, DownloadRecord, HlsMeta, ProbeMeta, SegmentRecord};

const COLS: &str =
    "id,url,final_url,filename,save_dir,size,mime,category,state,error_kind,error_msg,\
priority,max_connections,speed_limit,etag,last_modified,checksum,kind,page_url,page_title,\
filename_locked,created_at,started_at,finished_at,auto_place,pause_reason";

fn to_record(r: &Row) -> rusqlite::Result<DownloadRecord> {
    let state: String = r.get(8)?;
    Ok(DownloadRecord {
        id: r.get(0)?,
        url: r.get(1)?,
        final_url: r.get(2)?,
        filename: r.get(3)?,
        save_dir: r.get(4)?,
        size: r.get::<_, Option<i64>>(5)?.map(|v| v as u64),
        mime: r.get(6)?,
        category: r.get(7)?,
        state: DownloadState::parse(&state).unwrap_or(DownloadState::Failed),
        error_kind: r.get(9)?,
        error_msg: r.get(10)?,
        priority: r.get(11)?,
        max_connections: r.get::<_, i64>(12)? as u32,
        speed_limit: r.get::<_, Option<i64>>(13)?.map(|v| v as u64),
        etag: r.get(14)?,
        last_modified: r.get(15)?,
        checksum: r.get(16)?,
        kind: r.get(17)?,
        page_url: r.get(18)?,
        page_title: r.get(19)?,
        filename_locked: r.get::<_, i64>(20)? != 0,
        created_at: r.get(21)?,
        started_at: r.get(22)?,
        finished_at: r.get(23)?,
        auto_place: r.get::<_, i64>(24)? != 0,
        pause_reason: r.get(25)?,
    })
}

pub fn insert_download(c: &Connection, d: &DownloadRecord) -> rusqlite::Result<()> {
    c.execute(
        &format!(
            "INSERT INTO downloads ({COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26)"
        ),
        params![
            d.id, d.url, d.final_url, d.filename, d.save_dir, d.size.map(|v| v as i64), d.mime,
            d.category, d.state.as_str(), d.error_kind, d.error_msg, d.priority,
            d.max_connections as i64, d.speed_limit.map(|v| v as i64), d.etag, d.last_modified,
            d.checksum, d.kind, d.page_url, d.page_title, d.filename_locked as i64, d.created_at,
            d.started_at, d.finished_at, d.auto_place as i64, d.pause_reason
        ],
    )?;
    Ok(())
}

pub fn get_download(c: &Connection, id: &str) -> rusqlite::Result<Option<DownloadRecord>> {
    c.query_row(
        &format!("SELECT {COLS} FROM downloads WHERE id=?1"),
        [id],
        to_record,
    )
    .optional()
}

pub fn list_downloads(c: &Connection) -> rusqlite::Result<Vec<DownloadRecord>> {
    let mut st = c.prepare(&format!(
        "SELECT {COLS} FROM downloads ORDER BY created_at DESC, rowid DESC"
    ))?;
    let rows = st.query_map([], to_record)?;
    rows.collect()
}

pub fn list_by_state(c: &Connection, s: DownloadState) -> rusqlite::Result<Vec<DownloadRecord>> {
    let mut st = c.prepare(&format!(
        "SELECT {COLS} FROM downloads WHERE state=?1 ORDER BY priority DESC, created_at ASC, rowid ASC"
    ))?;
    let rows = st.query_map([s.as_str()], to_record)?;
    rows.collect()
}

pub fn set_state(
    c: &Connection,
    id: &str,
    s: DownloadState,
    err: Option<(String, String)>,
) -> rusqlite::Result<()> {
    let (kind, msg) = match err {
        Some((k, m)) => (Some(k), Some(m)),
        None => (None, None),
    };
    c.execute(
        "UPDATE downloads SET state=?2, error_kind=?3, error_msg=?4, pause_reason=NULL,
            started_at = CASE WHEN ?2='downloading' AND started_at IS NULL THEN ?5 ELSE started_at END,
            finished_at = CASE WHEN ?2 IN ('completed','failed','cancelled') THEN ?5
                               WHEN ?2='queued' THEN NULL ELSE finished_at END
         WHERE id=?1",
        params![id, s.as_str(), kind, msg, now_ms()],
    )?;
    Ok(())
}

pub fn update_probe(c: &Connection, id: &str, m: &ProbeMeta) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE downloads SET
            final_url = COALESCE(?2, final_url),
            size = COALESCE(?3, size),
            mime = COALESCE(?4, mime),
            etag = COALESCE(?5, etag),
            last_modified = COALESCE(?6, last_modified),
            filename = CASE WHEN filename_locked = 0 AND ?7 IS NOT NULL THEN ?7 ELSE filename END
         WHERE id=?1",
        params![
            id,
            m.final_url,
            m.size.map(|v| v as i64),
            m.mime,
            m.etag,
            m.last_modified,
            m.filename
        ],
    )?;
    Ok(())
}

pub fn set_filename(c: &Connection, id: &str, name: &str) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE downloads SET filename=?2 WHERE id=?1",
        params![id, name],
    )?;
    Ok(())
}

pub fn set_headers(c: &mut Connection, id: &str, h: &[(String, String)]) -> rusqlite::Result<()> {
    let tx = c.transaction()?;
    tx.execute("DELETE FROM download_headers WHERE download_id=?1", [id])?;
    for (n, v) in h {
        tx.execute(
            "INSERT OR REPLACE INTO download_headers (download_id,name,value) VALUES (?1,?2,?3)",
            params![id, n, v],
        )?;
    }
    tx.commit()
}

pub fn get_headers(c: &Connection, id: &str) -> rusqlite::Result<Vec<(String, String)>> {
    let mut st =
        c.prepare("SELECT name,value FROM download_headers WHERE download_id=?1 ORDER BY rowid")?;
    let rows = st.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

pub fn replace_chunks(
    c: &mut Connection,
    id: &str,
    chunks: &[ChunkRecord],
) -> rusqlite::Result<()> {
    let tx = c.transaction()?;
    tx.execute("DELETE FROM chunks WHERE download_id=?1", [id])?;
    for ch in chunks {
        tx.execute(
            "INSERT INTO chunks (download_id,idx,start,end_incl,written) VALUES (?1,?2,?3,?4,?5)",
            params![
                id,
                ch.idx,
                ch.start as i64,
                ch.end.map(|v| v as i64),
                ch.written as i64
            ],
        )?;
    }
    tx.commit()
}

pub fn save_progress(c: &mut Connection, id: &str, p: &[(u32, u64)]) -> rusqlite::Result<()> {
    let tx = c.transaction()?;
    for (idx, w) in p {
        tx.execute(
            "UPDATE chunks SET written=?3 WHERE download_id=?1 AND idx=?2",
            params![id, idx, *w as i64],
        )?;
    }
    tx.commit()
}

pub fn get_chunks(c: &Connection, id: &str) -> rusqlite::Result<Vec<ChunkRecord>> {
    let mut st = c.prepare(
        "SELECT idx,start,end_incl,written FROM chunks WHERE download_id=?1 ORDER BY start",
    )?;
    let rows = st.query_map([id], |r| {
        Ok(ChunkRecord {
            idx: r.get::<_, i64>(0)? as u32,
            start: r.get::<_, i64>(1)? as u64,
            end: r.get::<_, Option<i64>>(2)?.map(|v| v as u64),
            written: r.get::<_, i64>(3)? as u64,
        })
    })?;
    rows.collect()
}

pub fn delete_download(c: &Connection, id: &str) -> rusqlite::Result<()> {
    c.execute("DELETE FROM downloads WHERE id=?1", [id])?;
    Ok(())
}

pub fn reset_interrupted(c: &Connection) -> rusqlite::Result<usize> {
    c.execute(
        "UPDATE downloads SET state='queued' WHERE state IN ('probing','downloading','merging')",
        [],
    )
}

const BYTES: &str = "COALESCE((SELECT SUM(written) FROM chunks WHERE download_id = downloads.id), 0) + COALESCE((SELECT SUM(bytes) FROM hls_segments WHERE download_id = downloads.id AND state = 'done'), 0)";

pub fn insert_download_with_headers(
    c: &mut Connection,
    d: &DownloadRecord,
    h: &[(String, String)],
) -> rusqlite::Result<()> {
    let tx = c.transaction()?;
    insert_download(&tx, d)?;
    for (n, v) in h {
        tx.execute(
            "INSERT OR REPLACE INTO download_headers (download_id,name,value) VALUES (?1,?2,?3)",
            params![d.id, n, v],
        )?;
    }
    tx.commit()
}

pub fn list_with_bytes(c: &Connection) -> rusqlite::Result<Vec<(DownloadRecord, u64)>> {
    let mut st = c.prepare(&format!(
        "SELECT {COLS}, {BYTES} FROM downloads ORDER BY created_at DESC, rowid DESC"
    ))?;
    let rows = st.query_map([], |r| Ok((to_record(r)?, r.get::<_, i64>(26)? as u64)))?;
    rows.collect()
}

pub fn get_with_bytes(c: &Connection, id: &str) -> rusqlite::Result<Option<(DownloadRecord, u64)>> {
    c.query_row(
        &format!("SELECT {COLS}, {BYTES} FROM downloads WHERE id=?1"),
        [id],
        |r| Ok((to_record(r)?, r.get::<_, i64>(26)? as u64)),
    )
    .optional()
}

pub fn set_placement(
    c: &Connection,
    id: &str,
    category: &str,
    save_dir: &str,
) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE downloads SET category=?2, save_dir=?3 WHERE id=?1",
        params![id, category, save_dir],
    )?;
    Ok(())
}

/// Changes the name and/or folder. A given name is locked against the probe; a given folder ends auto-placement.
pub fn set_target(
    c: &Connection,
    id: &str,
    filename: Option<&str>,
    save_dir: Option<&str>,
) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE downloads SET
            filename = COALESCE(?2, filename),
            filename_locked = CASE WHEN ?2 IS NOT NULL THEN 1 ELSE filename_locked END,
            save_dir = COALESCE(?3, save_dir),
            auto_place = CASE WHEN ?3 IS NOT NULL THEN 0 ELSE auto_place END
         WHERE id=?1",
        params![id, filename, save_dir],
    )?;
    Ok(())
}

pub fn count_by_state(c: &Connection) -> rusqlite::Result<Vec<(DownloadState, u64)>> {
    let mut st = c.prepare("SELECT state, COUNT(*) FROM downloads GROUP BY state")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (state, n) = row?;
        if let Some(s) = DownloadState::parse(&state) {
            out.push((s, n as u64));
        }
    }
    Ok(out)
}

/// Replaces the link. The stored validators stay so a resume can still verify
/// that the new link serves the same file.
pub fn set_url(c: &Connection, id: &str, url: &str) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE downloads SET url=?2, final_url=NULL WHERE id=?1",
        params![id, url],
    )?;
    Ok(())
}

pub fn replace_segments(
    c: &mut Connection,
    id: &str,
    segs: &[SegmentRecord],
) -> rusqlite::Result<()> {
    let tx = c.transaction()?;
    tx.execute("DELETE FROM hls_segments WHERE download_id=?1", [id])?;
    for s in segs {
        tx.execute(
            "INSERT INTO hls_segments (download_id,idx,url,duration_ms,state,bytes) VALUES (?1,?2,?3,?4,?5,?6)",
            params![id, s.idx, s.url, s.duration_ms as i64, s.state, s.bytes as i64],
        )?;
    }
    tx.commit()
}

pub fn get_segments(c: &Connection, id: &str) -> rusqlite::Result<Vec<SegmentRecord>> {
    let mut st = c.prepare(
        "SELECT idx,url,duration_ms,state,bytes FROM hls_segments WHERE download_id=?1 ORDER BY idx",
    )?;
    let rows = st.query_map([id], |r| {
        Ok(SegmentRecord {
            idx: r.get::<_, i64>(0)? as u32,
            url: r.get(1)?,
            duration_ms: r.get::<_, i64>(2)? as u64,
            state: r.get(3)?,
            bytes: r.get::<_, i64>(4)? as u64,
        })
    })?;
    rows.collect()
}

pub fn mark_segment_done(c: &Connection, id: &str, idx: u32, bytes: u64) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE hls_segments SET state='done', bytes=?3 WHERE download_id=?1 AND idx=?2",
        params![id, idx, bytes as i64],
    )?;
    Ok(())
}

pub fn set_hls_meta(c: &Connection, id: &str, m: &HlsMeta) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO hls_meta (download_id,variant_url,key_url,iv,map_url) VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT(download_id) DO UPDATE SET
            variant_url=excluded.variant_url, key_url=excluded.key_url,
            iv=excluded.iv, map_url=excluded.map_url",
        params![id, m.variant_url, m.key_url, m.iv, m.map_url],
    )?;
    Ok(())
}

pub fn get_hls_meta(c: &Connection, id: &str) -> rusqlite::Result<Option<HlsMeta>> {
    c.query_row(
        "SELECT variant_url,key_url,iv,map_url FROM hls_meta WHERE download_id=?1",
        [id],
        |r| {
            Ok(HlsMeta {
                variant_url: r.get(0)?,
                key_url: r.get(1)?,
                iv: r.get(2)?,
                map_url: r.get(3)?,
            })
        },
    )
    .optional()
}

/// Pauses with a reason (`None` = by the user). Any later `set_state` clears the reason.
pub fn set_paused(c: &Connection, id: &str, reason: Option<&str>) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE downloads SET state='paused', error_kind=NULL, error_msg=NULL, pause_reason=?2
         WHERE id=?1",
        params![id, reason],
    )?;
    Ok(())
}

/// Deletes the records (never the files) of downloads completed before `cutoff_ms`.
/// Returns their ids.
pub fn delete_completed_before(
    c: &mut Connection,
    cutoff_ms: i64,
) -> rusqlite::Result<Vec<String>> {
    let tx = c.transaction()?;
    let ids: Vec<String> = {
        let mut st = tx.prepare(
            "SELECT id FROM downloads WHERE state='completed' AND finished_at IS NOT NULL
             AND finished_at < ?1 ORDER BY finished_at",
        )?;
        let rows = st.query_map([cutoff_ms], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for id in &ids {
        tx.execute("DELETE FROM downloads WHERE id=?1", [id])?;
    }
    tx.commit()?;
    Ok(ids)
}

/// Bytes per second; None = no limit of its own.
pub fn set_speed_limit(c: &Connection, id: &str, bps: Option<u64>) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE downloads SET speed_limit=?2 WHERE id=?1",
        params![id, bps.map(|v| v as i64)],
    )?;
    Ok(())
}
