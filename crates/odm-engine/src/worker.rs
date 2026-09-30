use futures_util::StreamExt;
use odm_core::{ErrorKind, OdmError};
use reqwest::header::{self, HeaderMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::chunks::ChunkRef;
use crate::retry::{classify_status, http_err, io_err, parse_retry_after};
use crate::SpeedLimiter;

#[derive(Clone)]
pub struct ChunkCtx {
    pub client: reqwest::Client,
    pub url: String,
    pub headers: HeaderMap,
    pub validator: Option<String>,
    pub file: Arc<std::fs::File>,
    pub global_limit: Arc<SpeedLimiter>,
    pub local_limit: Option<Arc<SpeedLimiter>>,
    pub cancel: CancellationToken,
    pub require_partial: bool,
    pub session_bytes: Arc<AtomicU64>,
}

#[derive(Debug, Clone)]
pub struct ChunkFailure {
    pub error: OdmError,
    pub retry_after: Option<Duration>,
}

impl From<OdmError> for ChunkFailure {
    fn from(error: OdmError) -> Self {
        Self {
            error,
            retry_after: None,
        }
    }
}

fn fail(kind: ErrorKind, msg: impl Into<String>) -> ChunkFailure {
    OdmError::new(kind, msg).into()
}

pub fn write_at(file: &std::fs::File, mut offset: u64, mut buf: &[u8]) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            let n = file.seek_write(buf, offset)?;
            if n == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::WriteZero));
            }
            buf = &buf[n..];
            offset += n as u64;
        }
        Ok(())
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.write_all_at(buf, offset)
    }
}

/// Parses the start offset of "bytes a-b/total".
fn content_range_start(h: &HeaderMap) -> Option<u64> {
    let v = h.get(header::CONTENT_RANGE)?.to_str().ok()?;
    let rest = v.trim().strip_prefix("bytes ")?;
    rest.split('-').next()?.trim().parse().ok()
}

pub async fn run_chunk(ctx: &ChunkCtx, chunk: &ChunkRef) -> Result<(), ChunkFailure> {
    let (offset, end) = {
        let mut c = chunk.lock().unwrap();
        c.claimed = c.written; // discard un-flushed reservations from a failed attempt
        if c.is_done() {
            return Ok(());
        }
        (c.start + c.written, c.end)
    };

    let send_range = ctx.require_partial || offset > 0;
    let mut req = ctx.client.get(&ctx.url).headers(ctx.headers.clone());
    if send_range {
        let r = match end {
            Some(e) => format!("bytes={offset}-{e}"),
            None => format!("bytes={offset}-"),
        };
        req = req.header(header::RANGE, r);
        if let Some(v) = &ctx.validator {
            req = req.header(header::IF_RANGE, v);
        }
    }

    let resp = tokio::select! {
        _ = ctx.cancel.cancelled() => return Err(fail(ErrorKind::Cancelled, "cancelled")),
        r = req.send() => r.map_err(|e| ChunkFailure::from(http_err(e)))?,
    };

    let status = resp.status().as_u16();
    match status {
        206 => {
            if content_range_start(resp.headers()) != Some(offset) {
                return Err(fail(
                    ErrorKind::RangeIgnored,
                    "206 with unexpected Content-Range",
                ));
            }
        }
        200 if send_range && ctx.validator.is_some() => {
            return Err(fail(
                ErrorKind::FileChanged,
                "server file changed (If-Range mismatch)",
            ));
        }
        200 if send_range => return Err(fail(ErrorKind::RangeIgnored, "server ignored Range")),
        200 => {}
        s => {
            let retry_after = resp
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after);
            return Err(ChunkFailure {
                error: OdmError::new(classify_status(s, true), format!("HTTP {s}")),
                retry_after,
            });
        }
    }

    let mut stream = resp.bytes_stream();
    loop {
        let next = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(fail(ErrorKind::Cancelled, "cancelled")),
            n = stream.next() => n,
        };
        let data = match next {
            None => break,
            Some(Err(e)) => return Err(http_err(e).into()),
            Some(Ok(d)) => d,
        };

        tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(fail(ErrorKind::Cancelled, "cancelled")),
            _ = async {
                ctx.global_limit.acquire(data.len()).await;
                if let Some(l) = &ctx.local_limit { l.acquire(data.len()).await; }
            } => {}
        }

        let (off, allowed) = chunk.lock().unwrap().claim(data.len());
        if allowed > 0 {
            // Positioned writes can block (disk, NTFS valid-data extension), so run
            // them off the async runtime. The write is awaited to completion rather
            // than abandoned on cancel: an orphaned write could race a restart that
            // removes and recreates the part file. Cancellation is re-checked on the
            // next loop iteration, so its latency is bounded by one buffer write.
            let file = ctx.file.clone();
            let buf = data.slice(..allowed);
            tokio::task::spawn_blocking(move || write_at(&file, off, &buf))
                .await
                .map_err(|e| fail(ErrorKind::Other, format!("write task failed: {e}")))?
                .map_err(|e| ChunkFailure::from(io_err(e)))?;
            // Count bytes only after their write returned Ok.
            chunk.lock().unwrap().written += allowed as u64;
            ctx.session_bytes
                .fetch_add(allowed as u64, Ordering::Relaxed);
        }
        if allowed < data.len() {
            return Ok(()); // reached this chunk's end (possibly moved by a split)
        }
    }

    let mut c = chunk.lock().unwrap();
    match c.end {
        None => {
            if c.written > 0 {
                c.end = Some(c.start + c.written - 1);
            }
            Ok(())
        }
        Some(_) if c.is_done() => Ok(()),
        Some(_) => Err(fail(
            ErrorKind::Connection,
            "server closed the connection early",
        )),
    }
}
