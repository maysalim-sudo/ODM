//! A static-file HTTP server for HLS tests, plus helpers that write playlists.
use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use axum::{
    body::Body,
    extract::{Path as UrlPath, State},
    http::{header, Response},
    routing::get,
    Router,
};
use bytes::Bytes;
use futures_util::stream;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Default)]
pub struct HlsFaults {
    /// Throttle every response body to this rate (per connection).
    pub bytes_per_sec: Option<u64>,
    /// Segment `s<idx>.ts` (1-based) is answered 503 once.
    pub fail_segment_once: Option<u32>,
}

pub struct HlsServer {
    pub base: String,
    /// Requests for `.ts` / `.m4s` files (every attempt, including refused ones).
    pub segment_requests: Arc<AtomicU32>,
}

struct Inner {
    dir: PathBuf,
    faults: HlsFaults,
    segment_requests: Arc<AtomicU32>,
    failed_once: AtomicBool,
}

fn content_type(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "m3u8" => "application/vnd.apple.mpegurl",
        "ts" => "video/mp2t",
        "m4s" | "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

fn not_found() -> Response<Body> {
    Response::builder().status(404).body(Body::empty()).unwrap()
}

async fn serve(State(s): State<Arc<Inner>>, UrlPath(rel): UrlPath<String>) -> Response<Body> {
    let rel_path = Path::new(&rel);
    if rel_path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return not_found();
    }
    let is_segment = rel.ends_with(".ts") || rel.ends_with(".m4s");
    if is_segment {
        s.segment_requests.fetch_add(1, Ordering::SeqCst);
        if let Some(idx) = s.faults.fail_segment_once {
            if rel == format!("s{idx}.ts") && !s.failed_once.swap(true, Ordering::SeqCst) {
                return Response::builder().status(503).body(Body::empty()).unwrap();
            }
        }
    }
    let data = match tokio::fs::read(s.dir.join(rel_path)).await {
        Ok(d) => d,
        Err(_) => return not_found(),
    };
    let b = Response::builder()
        .status(200)
        .header(header::CONTENT_TYPE, content_type(&rel))
        .header(header::CONTENT_LENGTH, data.len().to_string());
    let piece = 16 * 1024;
    let chunks: Vec<Bytes> = data.chunks(piece).map(Bytes::copy_from_slice).collect();
    let rate = s.faults.bytes_per_sec;
    let st = stream::unfold(chunks.into_iter(), move |mut it| async move {
        let c = it.next()?;
        if let Some(r) = rate {
            tokio::time::sleep(Duration::from_secs_f64(c.len() as f64 / r as f64)).await;
        }
        Some((Ok::<_, std::io::Error>(c), it))
    });
    b.body(Body::from_stream(st)).unwrap()
}

impl HlsServer {
    /// Serves files from `dir` at "/<relative path>". 404 for missing files.
    pub async fn start(dir: PathBuf, faults: HlsFaults) -> HlsServer {
        let segment_requests = Arc::new(AtomicU32::new(0));
        let inner = Arc::new(Inner {
            dir,
            faults,
            segment_requests: segment_requests.clone(),
            failed_once: AtomicBool::new(false),
        });
        let app = Router::new()
            .route("/{*path}", get(serve))
            .with_state(inner);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        HlsServer {
            base: format!("http://{addr}"),
            segment_requests,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }
}

/// Writes a synthetic VOD playlist "media.m3u8" with `n` segments "s1.ts".."s<n>.ts" of `seg_len`
/// pattern bytes each (byte i of segment k = (k*31 + i) % 251), optionally AES-128 encrypted with
/// key file "key.bin" (16 bytes 0..15) and an IV derived from the sequence number.
/// Returns the plaintext of every segment, in order.
pub fn write_synthetic(dir: &Path, n: u32, seg_len: usize, encrypt: bool) -> Vec<Vec<u8>> {
    std::fs::create_dir_all(dir).unwrap();
    let key: [u8; 16] = std::array::from_fn(|i| i as u8);
    if encrypt {
        std::fs::write(dir.join("key.bin"), key).unwrap();
    }
    let mut playlist = String::from("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:4\n");
    playlist.push_str("#EXT-X-MEDIA-SEQUENCE:0\n");
    if encrypt {
        playlist.push_str("#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n");
    }
    let mut plains = Vec::new();
    for k in 1..=n {
        let plain: Vec<u8> = (0..seg_len)
            .map(|i| ((k as usize * 31 + i) % 251) as u8)
            .collect();
        let on_disk = if encrypt {
            let mut iv = [0u8; 16];
            iv[8..].copy_from_slice(&((k - 1) as u64).to_be_bytes());
            cbc::Encryptor::<aes::Aes128>::new((&key).into(), (&iv).into())
                .encrypt_padded_vec_mut::<Pkcs7>(&plain)
        } else {
            plain.clone()
        };
        std::fs::write(dir.join(format!("s{k}.ts")), on_disk).unwrap();
        playlist.push_str(&format!("#EXTINF:4.0,\ns{k}.ts\n"));
        plains.push(plain);
    }
    playlist.push_str("#EXT-X-ENDLIST\n");
    std::fs::write(dir.join("media.m3u8"), playlist).unwrap();
    plains
}

/// Uses ffmpeg (if found on PATH) to write a real 4-second VOD stream "real/index.m3u8" with
/// 1-second TS segments; returns false when ffmpeg is unavailable or fails.
pub fn write_real_ts(dir: &Path) -> bool {
    let out = dir.join("real");
    if std::fs::create_dir_all(&out).is_err() {
        return false;
    }
    std::process::Command::new("ffmpeg")
        .current_dir(dir)
        .args([
            "-hide_banner",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=4:size=160x120:rate=10",
            "-f",
            "lavfi",
            "-i",
            "sine=duration=4",
            "-c:v",
            "libx264",
            "-g",
            "10",
            "-bf",
            "0",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-f",
            "hls",
            "-hls_time",
            "1",
            "-hls_playlist_type",
            "vod",
            "real/index.m3u8",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
