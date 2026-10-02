use odm_core::{category, filename};
use odm_engine::download::build_client;
use odm_engine::{AddRequest, EngineConfig, ManagerHandle};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::dto::{chunk_views, to_dto, DetailDto, DownloadDto};
use crate::Settings;

/// A plain-language message for the UI. Never shows internal codes.
pub fn user_message(e: &odm_core::OdmError) -> String {
    use odm_core::ErrorKind::*;
    match e.kind {
        Http(404) => "The server could not find this file (error 404).".into(),
        Http(n @ (401 | 403)) => {
            format!("The server refused access (error {n}). You may need to sign in on the website first.")
        }
        Http(n) | HttpTransient(n) => format!("The server returned an error ({n})."),
        Timeout => "The server took too long to respond.".into(),
        Connection | Dns => {
            "Could not connect to the server. Check the address and your internet connection."
                .into()
        }
        DiskFull => "The disk is full.".into(),
        PermissionDenied => "ODM is not allowed to write to that folder.".into(),
        LinkExpired => "This link has expired. Start the download again from the website.".into(),
        FileChanged => "The file changed on the server while it was downloading.".into(),
        ChecksumMismatch => "The downloaded file does not match the expected checksum.".into(),
        RangeIgnored => "The server does not support resuming this download.".into(),
        Cancelled => "Cancelled.".into(),
        Io | Other => {
            let m = e.message.trim();
            let mut chars = m.chars();
            let mut out: String = match chars.next() {
                Some(c) => c.to_uppercase().chain(chars).collect(),
                None => "Something went wrong.".into(),
            };
            if !out.ends_with('.') {
                out.push('.');
            }
            out
        }
    }
}

fn msg(e: odm_core::OdmError) -> String {
    user_message(&e)
}

fn given(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

pub async fn list(m: &ManagerHandle) -> Result<Vec<DownloadDto>, String> {
    Ok(m.list()
        .await
        .map_err(msg)?
        .iter()
        .map(|(r, b)| to_dto(r, *b))
        .collect())
}

pub async fn detail(m: &ManagerHandle, id: &str) -> Result<Option<DetailDto>, String> {
    let Some((rec, bytes)) = m.get(id).await.map_err(msg)? else {
        return Ok(None);
    };
    Ok(Some(DetailDto {
        download: to_dto(&rec, bytes),
        final_url: rec.final_url.clone(),
        chunks: chunk_views(&m.chunks(id).await.map_err(msg)?),
        header_names: m.header_names(id).await.map_err(msg)?,
    }))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AddArgs {
    pub url: String,
    pub filename: Option<String>,
    pub save_dir: Option<String>,
    pub connections: Option<u32>,
    pub category: Option<String>,
    pub checksum: Option<String>,
    pub start_paused: bool,
}

pub async fn add(m: &ManagerHandle, a: AddArgs) -> Result<String, String> {
    let mut req = AddRequest::new(a.url.trim());
    req.filename = given(a.filename);
    req.save_dir = given(a.save_dir).map(PathBuf::from);
    req.category = given(a.category);
    req.checksum = given(a.checksum);
    req.max_connections = a.connections;
    req.start_paused = a.start_paused;
    m.add(req).await.map_err(msg)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParsedUrls {
    pub valid: Vec<String>,
    pub rejected: Vec<String>,
}

fn is_http_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    ["http://", "https://"].iter().any(|p| {
        lower
            .strip_prefix(p)
            .is_some_and(|rest| !rest.is_empty() && !rest.starts_with('/'))
    })
}

/// One URL per line. Blank lines are skipped; duplicates are kept once.
pub fn parse_urls(text: &str) -> ParsedUrls {
    let mut out = ParsedUrls {
        valid: Vec::new(),
        rejected: Vec::new(),
    };
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let bucket = if is_http_url(line) {
            &mut out.valid
        } else {
            &mut out.rejected
        };
        if !bucket.iter().any(|x| x == line) {
            bucket.push(line.to_string());
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatchResult {
    pub added: Vec<String>,
    pub rejected: Vec<String>,
}

pub async fn add_batch(
    m: &ManagerHandle,
    text: &str,
    save_dir: Option<String>,
) -> Result<BatchResult, String> {
    let parsed = parse_urls(text);
    let save_dir = given(save_dir);
    let mut added = Vec::new();
    let mut rejected = parsed.rejected;
    for url in parsed.valid {
        let args = AddArgs {
            url: url.clone(),
            save_dir: save_dir.clone(),
            ..Default::default()
        };
        match add(m, args).await {
            Ok(id) => added.push(id),
            Err(_) => rejected.push(url),
        }
    }
    Ok(BatchResult { added, rejected })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Suggestion {
    pub filename: String,
    pub size: Option<u64>,
    pub category: String,
    pub resumable: bool,
}

/// Ask the server what this URL is, to pre-fill the Add dialog.
pub async fn suggest(url: &str, cfg: &EngineConfig) -> Result<Suggestion, String> {
    let url = url.trim();
    let client = build_client(cfg);
    let p = odm_engine::probe(&client, url, &Default::default())
        .await
        .map_err(msg)?;
    let name = p
        .filename
        .clone()
        .or_else(|| filename::from_url(&p.final_url))
        .or_else(|| filename::from_url(url))
        .unwrap_or_else(|| "download".into());
    Ok(Suggestion {
        category: category::detect(&name, p.mime.as_deref())
            .as_str()
            .to_string(),
        filename: name,
        size: p.size,
        resumable: p.supports_range,
    })
}

pub async fn apply_settings(m: &ManagerHandle, s: &Settings) -> Result<(), String> {
    m.set_config(s.to_engine_config()).await.map_err(msg)
}

#[cfg(test)]
mod tests {
    use super::user_message;
    use odm_core::{ErrorKind::*, OdmError};

    fn m(k: odm_core::ErrorKind, text: &str) -> String {
        user_message(&OdmError::new(k, text))
    }

    #[test]
    fn every_error_kind_has_a_plain_message() {
        assert_eq!(
            m(Http(404), "x"),
            "The server could not find this file (error 404)."
        );
        for n in [401, 403] {
            assert_eq!(
                m(Http(n), "x"),
                format!("The server refused access (error {n}). You may need to sign in on the website first.")
            );
        }
        assert_eq!(m(Http(410), "x"), "The server returned an error (410).");
        assert_eq!(
            m(HttpTransient(503), "x"),
            "The server returned an error (503)."
        );
        assert_eq!(m(Timeout, "x"), "The server took too long to respond.");
        for k in [Connection, Dns] {
            assert_eq!(
                m(k, "x"),
                "Could not connect to the server. Check the address and your internet connection."
            );
        }
        assert_eq!(m(DiskFull, "x"), "The disk is full.");
        assert_eq!(
            m(PermissionDenied, "x"),
            "ODM is not allowed to write to that folder."
        );
        assert_eq!(
            m(LinkExpired, "x"),
            "This link has expired. Start the download again from the website."
        );
        assert_eq!(
            m(FileChanged, "x"),
            "The file changed on the server while it was downloading."
        );
        assert_eq!(
            m(ChecksumMismatch, "x"),
            "The downloaded file does not match the expected checksum."
        );
        assert_eq!(
            m(RangeIgnored, "x"),
            "The server does not support resuming this download."
        );
        assert_eq!(m(Cancelled, "x"), "Cancelled.");
        assert_eq!(m(Io, "disk went away"), "Disk went away.");
        assert_eq!(m(Other, "Already done."), "Already done.");
        assert_eq!(m(Other, ""), "Something went wrong.");
    }
}
