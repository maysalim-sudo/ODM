//! Fault-injecting HTTP server for engine tests. Not shipped.
pub mod files;
pub mod hls;
pub mod proxy;

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, Response, StatusCode},
    routing::get,
    Router,
};
use bytes::Bytes;
use futures_util::stream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub fn body(len: u64) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

#[derive(Clone, Default)]
pub struct Faults {
    pub no_ranges: bool,
    pub no_length: bool,
    pub drop_after: Option<u64>,
    pub drop_times: u32,
    pub fail_status: Option<u16>,
    pub fail_times: u32,
    pub etag_changes_after: Option<u32>,
    pub forbid_after: Option<u32>,
    pub wrong_range_start: bool,
    pub bytes_per_sec: Option<u64>,
    pub filename: Option<String>,
    /// `/file` answers every request with a 302 to this absolute URL.
    pub redirect_to: Option<String>,
    /// Answer `bytes=0-0` (the probe) with 206, but any other Range with a 200
    /// full body carrying the same ETag.
    pub only_probe_range: bool,
    /// Content-Type of `/file` answers; `None` means `application/octet-stream`.
    pub content_type: Option<String>,
    /// A `/file` request arriving while this many response bodies are already in
    /// flight gets `429` + `Retry-After: 1`. The probe (`Range: bytes=0-0`) is
    /// never refused.
    pub max_parallel: Option<u32>,
}

struct Inner {
    data: Arc<Vec<u8>>,
    faults: Faults,
    requests: Arc<AtomicU32>,
    cookie_requests: AtomicU32,
    in_flight: AtomicU32,
    refused: AtomicU32,
    max_in_flight: AtomicU32,
}

pub struct TestServer {
    pub base: String,
    pub requests: Arc<AtomicU32>,
    inner: Arc<Inner>,
}

impl TestServer {
    pub async fn start(len: u64, faults: Faults) -> TestServer {
        let requests = Arc::new(AtomicU32::new(0));
        let inner = Arc::new(Inner {
            data: Arc::new(body(len)),
            faults,
            requests: requests.clone(),
            cookie_requests: AtomicU32::new(0),
            in_flight: AtomicU32::new(0),
            refused: AtomicU32::new(0),
            max_in_flight: AtomicU32::new(0),
        });
        let app = Router::new()
            .route("/file", get(file))
            .route("/empty", get(empty))
            .route(
                "/redirect",
                get(|| async {
                    Response::builder()
                        .status(302)
                        .header(header::LOCATION, "/file")
                        .body(Body::empty())
                        .unwrap()
                }),
            )
            .with_state(inner.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        TestServer {
            base: format!("http://{addr}"),
            requests,
            inner,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    pub fn max_concurrent(&self) -> u32 {
        self.inner.max_in_flight.load(Ordering::SeqCst)
    }

    /// Number of `/file` requests answered 429 by the `max_parallel` fault.
    pub fn refused(&self) -> u32 {
        self.inner.refused.load(Ordering::SeqCst)
    }

    /// Number of `/file` requests that carried a `Cookie` header.
    pub fn cookie_requests(&self) -> u32 {
        self.inner.cookie_requests.load(Ordering::SeqCst)
    }
}

async fn empty() -> Response<Body> {
    Response::builder()
        .status(200)
        .header(header::CONTENT_LENGTH, "0")
        .header(header::ACCEPT_RANGES, "bytes")
        .body(Body::empty())
        .unwrap()
}

fn parse_range(h: &HeaderMap, len: u64) -> Option<(u64, u64)> {
    if len == 0 {
        return None;
    }
    let v = h.get(header::RANGE)?.to_str().ok()?;
    let spec = v.strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    let start: u64 = a.parse().ok()?;
    let end: u64 = if b.is_empty() {
        len - 1
    } else {
        b.parse::<u64>().ok()?.min(len - 1)
    };
    (start <= end && start < len).then_some((start, end))
}

async fn file(State(s): State<Arc<Inner>>, h: HeaderMap) -> Response<Body> {
    let n = s.requests.fetch_add(1, Ordering::SeqCst) + 1;
    let f = &s.faults;
    let len = s.data.len() as u64;
    if h.contains_key(header::COOKIE) {
        s.cookie_requests.fetch_add(1, Ordering::SeqCst);
    }
    if let Some(to) = &f.redirect_to {
        return Response::builder()
            .status(302)
            .header(header::LOCATION, to.as_str())
            .body(Body::empty())
            .unwrap();
    }

    if let Some(code) = f.fail_status {
        if n <= f.fail_times {
            let mut b = Response::builder().status(code);
            if code == 429 {
                b = b.header(header::RETRY_AFTER, "1");
            }
            return b.body(Body::empty()).unwrap();
        }
    }
    if matches!(f.forbid_after, Some(k) if n > k) {
        return Response::builder().status(403).body(Body::empty()).unwrap();
    }

    let etag = match f.etag_changes_after {
        Some(k) if n > k => "\"v2\"",
        _ => "\"v1\"",
    };
    let if_range_ok = h
        .get(header::IF_RANGE)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|v| v == etag);

    let range_ignored = f.only_probe_range
        && h.get(header::RANGE)
            .is_some_and(|v| v.as_bytes() != b"bytes=0-0");
    let range = if f.no_ranges || !if_range_ok || range_ignored {
        None
    } else {
        parse_range(&h, len)
    };
    if !f.no_ranges
        && !range_ignored
        && h.contains_key(header::RANGE)
        && range.is_none()
        && if_range_ok
    {
        return Response::builder()
            .status(416)
            .header(header::CONTENT_RANGE, format!("bytes */{len}"))
            .body(Body::empty())
            .unwrap();
    }

    let (status, start, end) = match range {
        Some((a, b)) => (StatusCode::PARTIAL_CONTENT, a, b),
        None => (StatusCode::OK, 0, len.saturating_sub(1)),
    };
    let mut slice = if len == 0 {
        Vec::new()
    } else {
        s.data[start as usize..=end as usize].to_vec()
    };
    if let Some(k) = f.drop_after {
        if n <= f.drop_times {
            slice.truncate(k as usize);
        }
    }
    let dropping = f.drop_after.is_some() && n <= f.drop_times;

    let mut b = Response::builder()
        .status(status)
        .header(header::ETAG, etag)
        .header(
            header::CONTENT_TYPE,
            f.content_type
                .as_deref()
                .unwrap_or("application/octet-stream"),
        );
    if !f.no_ranges {
        b = b.header(header::ACCEPT_RANGES, "bytes");
    }
    if status == StatusCode::PARTIAL_CONTENT {
        let shown = if f.wrong_range_start {
            start + 1
        } else {
            start
        };
        b = b.header(header::CONTENT_RANGE, format!("bytes {shown}-{end}/{len}"));
    }
    if !f.no_length {
        // When dropping, advertise the full length so the client sees a short body.
        b = b.header(header::CONTENT_LENGTH, (end - start + 1).to_string());
    }
    if let Some(name) = &f.filename {
        b = b.header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{name}\""),
        );
    }

    let rate = f.bytes_per_sec;
    let inner = s.clone();
    let is_probe = h
        .get(header::RANGE)
        .is_some_and(|v| v.as_bytes() == b"bytes=0-0");
    // `fetch_update` is renamed `try_update` in Rust 1.99; keep the old name for older toolchains.
    #[allow(deprecated)]
    let cur = match f.max_parallel {
        // Reserve a slot atomically; a refused request is never counted.
        Some(limit) if !is_probe => {
            match inner
                .in_flight
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |c| {
                    (c < limit).then_some(c + 1)
                }) {
                Ok(prev) => prev + 1,
                Err(_) => {
                    inner.refused.fetch_add(1, Ordering::SeqCst);
                    return Response::builder()
                        .status(429)
                        .header(header::RETRY_AFTER, "1")
                        .body(Body::empty())
                        .unwrap();
                }
            }
        }
        _ => inner.in_flight.fetch_add(1, Ordering::SeqCst) + 1,
    };
    inner.max_in_flight.fetch_max(cur, Ordering::SeqCst);
    let guard = InFlight(inner);

    let piece = 16 * 1024;
    let chunks: Vec<Bytes> = slice.chunks(piece).map(Bytes::copy_from_slice).collect();
    let total = chunks.len();
    let st = stream::unfold(
        (0usize, chunks, guard),
        move |(i, chunks, guard)| async move {
            if i == usize::MAX {
                return None; // error already emitted
            }
            if i >= total {
                if dropping {
                    // Let hyper flush the head and the truncated body before the abort.
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    return Some((
                        Err(std::io::Error::other("dropped")),
                        (usize::MAX, chunks, guard),
                    ));
                }
                return None;
            }
            if let Some(r) = rate {
                tokio::time::sleep(Duration::from_secs_f64(chunks[i].len() as f64 / r as f64))
                    .await;
            }
            let c = chunks[i].clone();
            Some((Ok::<_, std::io::Error>(c), (i + 1, chunks, guard)))
        },
    );
    b.body(Body::from_stream(st)).unwrap()
}

/// Decrements the in-flight counter when the response stream is dropped
/// (finished, errored, or abandoned by a disconnecting client).
struct InFlight(Arc<Inner>);

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}
