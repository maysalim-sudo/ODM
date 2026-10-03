use odm_core::{category, filename};
use odm_engine::download::build_client;
use odm_engine::{AddRequest, EngineConfig, ManagerHandle};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::dto::{chunk_views, to_dto, DetailDto, DownloadDto};
use crate::offers::Offers;
use crate::Settings;

pub use crate::messages::user_message;

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

/// Gives a paused/failed download a new link (stored headers kept) and resumes it.
pub async fn refresh_url(m: &ManagerHandle, id: &str, url: &str) -> Result<(), String> {
    m.refresh_url(id, url.trim().to_string(), None)
        .await
        .map_err(msg)?;
    m.resume(id).await.map_err(msg)
}

/// reuse = true: refresh the existing download with the offer's link and headers, then resume it.
/// false: add the offered request as a new download. Returns the download id.
pub async fn resolve_offer(
    m: &ManagerHandle,
    offers: &Offers,
    offer_id: &str,
    reuse: bool,
) -> Result<String, String> {
    let (offer, req) = offers
        .take(offer_id)
        .ok_or_else(|| "This offer is no longer available.".to_string())?;
    if reuse {
        m.refresh_url(&offer.existing_id, req.url, Some(req.headers))
            .await
            .map_err(msg)?;
        m.resume(&offer.existing_id).await.map_err(msg)?;
        Ok(offer.existing_id)
    } else {
        m.add(req).await.map_err(msg)
    }
}

pub async fn apply_settings(m: &ManagerHandle, s: &Settings) -> Result<(), String> {
    m.set_config(s.to_engine_config()).await.map_err(msg)
}

/// A plain-language message for a database that could not be opened.
pub fn store_open_message(e: &odm_store::StoreError) -> String {
    match e {
        odm_store::StoreError::NewerSchema { .. } => {
            "This data was made by a newer version of ODM. Install the latest ODM, then try again."
                .into()
        }
        _ => {
            "ODM could not open its database. The file may be damaged or in use by another program."
                .into()
        }
    }
}

/// Clamp, write the file, then apply to the running manager.
pub async fn save_settings(
    m: &ManagerHandle,
    path: &std::path::Path,
    mut s: Settings,
) -> Result<(Settings, Vec<String>), String> {
    let warnings = s.clamp();
    s.save(path)
        .map_err(|e| format!("Could not save settings: {e}"))?;
    apply_settings(m, &s).await?;
    Ok((s, warnings))
}

#[cfg(test)]
mod tests {
    use super::store_open_message;

    #[test]
    fn store_open_messages_are_plain() {
        let newer = odm_store::StoreError::NewerSchema {
            found: 5,
            supported: 3,
        };
        assert!(store_open_message(&newer).starts_with("This data was made by a newer version"));
        let other = odm_store::StoreError::Closed;
        let text = store_open_message(&other);
        assert!(text.contains("damaged or in use"));
        assert!(!text.contains("sqlite"));
    }
}
