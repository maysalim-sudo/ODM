//! Turning what the user or the browser gave into a torrent source: magnet text, a
//! `.torrent` file on disk, or a `.torrent` URL fetched with the browser's headers.
use bytes::Bytes;
use futures_util::StreamExt;
use odm_torrent::TorrentSource;
use std::path::PathBuf;
use std::time::Duration;

pub const MAX_TORRENT_BYTES: usize = 10 * 1024 * 1024;
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

const NOT_VALID: &str = "This isn't a valid magnet link or torrent file.";
const TOO_LARGE: &str = "The torrent file is larger than 10 MB.";
const TOO_SLOW: &str = "The torrent file didn't download in time.";

#[derive(Debug, Clone, PartialEq)]
pub enum TorrentInput {
    /// A magnet link.
    Text(String),
    File(PathBuf),
    Url {
        url: String,
        headers: Vec<(String, String)>,
    },
}

/// The checked source, or the message to show. Nothing is added here.
pub async fn resolve(
    input: TorrentInput,
    http: &reqwest::Client,
    timeout: Duration,
) -> Result<TorrentSource, String> {
    let source = match input {
        TorrentInput::Text(t) => TorrentSource::Magnet(t.trim().to_string()),
        TorrentInput::File(p) => {
            let len = std::fs::metadata(&p).map_err(|e| e.to_string())?.len();
            if len > MAX_TORRENT_BYTES as u64 {
                return Err(TOO_LARGE.into());
            }
            TorrentSource::Bytes(std::fs::read(&p).map_err(|e| e.to_string())?.into())
        }
        TorrentInput::Url { url, headers } => {
            let bytes = tokio::time::timeout(timeout, fetch(http, &url, &headers))
                .await
                .map_err(|_| TOO_SLOW.to_string())??;
            TorrentSource::Bytes(bytes)
        }
    };
    odm_torrent::parse(&source).map_err(|_| NOT_VALID.to_string())?;
    Ok(source)
}

async fn fetch(
    http: &reqwest::Client,
    url: &str,
    headers: &[(String, String)],
) -> Result<Bytes, String> {
    let mut rq = http.get(url);
    for (k, v) in headers {
        rq = rq.header(k, v);
    }
    let resp = rq.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "The torrent file couldn't be downloaded (HTTP {}).",
            resp.status().as_u16()
        ));
    }
    if resp
        .content_length()
        .is_some_and(|l| l > MAX_TORRENT_BYTES as u64)
    {
        return Err(TOO_LARGE.into());
    }
    let mut out = Vec::new();
    let mut body = resp.bytes_stream();
    while let Some(chunk) = body.next().await {
        out.extend_from_slice(&chunk.map_err(|e| e.to_string())?);
        if out.len() > MAX_TORRENT_BYTES {
            return Err(TOO_LARGE.into());
        }
    }
    Ok(out.into())
}
