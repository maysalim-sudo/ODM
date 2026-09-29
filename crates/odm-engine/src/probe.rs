use odm_core::{filename, OdmError};
use reqwest::header::{self, HeaderMap};

use crate::retry::{classify_status, http_err};

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeResult {
    pub final_url: String,
    pub size: Option<u64>,
    pub supports_range: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub mime: Option<String>,
    pub filename: Option<String>,
}

impl ProbeResult {
    /// Value for If-Range: strong ETag preferred, else Last-Modified.
    pub fn validator(&self) -> Option<String> {
        self.etag
            .clone()
            .filter(|e| !e.starts_with("W/"))
            .or_else(|| self.last_modified.clone())
    }
}

pub fn parse_content_range_total(v: &str) -> Option<u64> {
    let rest = v.trim().strip_prefix("bytes ")?;
    let (_, total) = rest.split_once('/')?;
    total.trim().parse().ok()
}

fn header_str(h: &HeaderMap, name: header::HeaderName) -> Option<String> {
    h.get(name).and_then(|v| v.to_str().ok()).map(str::to_string)
}

pub async fn probe(client: &reqwest::Client, url: &str, headers: &HeaderMap) -> Result<ProbeResult, OdmError> {
    let resp = client
        .get(url)
        .headers(headers.clone())
        .header(header::RANGE, "bytes=0-0")
        .send()
        .await
        .map_err(http_err)?;
    let status = resp.status().as_u16();
    let h = resp.headers().clone();
    let final_url = resp.url().to_string();
    drop(resp); // never read the body of a probe

    let (size, supports_range) = match status {
        206 => (h.get(header::CONTENT_RANGE).and_then(|v| v.to_str().ok()).and_then(parse_content_range_total), true),
        416 => (
            Some(h.get(header::CONTENT_RANGE).and_then(|v| v.to_str().ok()).and_then(parse_content_range_total).unwrap_or(0)),
            true,
        ),
        200 => (h.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()), false),
        s => return Err(OdmError::new(classify_status(s, false), format!("probe returned HTTP {s}"))),
    };

    Ok(ProbeResult {
        final_url,
        size,
        supports_range,
        etag: header_str(&h, header::ETAG),
        last_modified: header_str(&h, header::LAST_MODIFIED),
        mime: header_str(&h, header::CONTENT_TYPE).map(|m| m.split(';').next().unwrap_or("").trim().to_string()),
        filename: header_str(&h, header::CONTENT_DISPOSITION).and_then(|v| filename::from_content_disposition(&v)),
    })
}
