//! The IPC command dispatcher: plain async logic against `ManagerHandle`, no transport.
use crate::capture::{normalize_headers, skip_reason};
use crate::confirm::Pending;
use crate::integration::Contact;
use crate::messages::user_message;
use crate::offers::{find_reusable, OfferDto, Offers};
use crate::summary::SpeedBoard;
use crate::Settings;
use odm_core::ipc::{
    cmd, code, AddDownload, AddPage, AddStream, AddTorrent, Added, PingReply, Request, Response,
    Source, Summary, TorrentLinkKind,
};
use odm_core::{filename, redact_url, DownloadState};
use odm_engine::{AddRequest, ManagerHandle, SiteRequest, StreamRequest};
use serde::Serialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct IpcContext {
    pub manager: ManagerHandle,
    pub settings: Arc<Mutex<Settings>>,
    pub speeds: Arc<Mutex<SpeedBoard>>,
    pub offers: Arc<Offers>,
    pub show_window: Box<dyn Fn() + Send + Sync>,
    pub on_offer: Box<dyn Fn(&OfferDto) + Send + Sync>,
    pub pending: Arc<Pending>,
    pub on_confirm: Box<dyn Fn(&str) + Send + Sync>,
    pub contact: Arc<Contact>,
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn reply<T: Serialize>(id: Option<String>, v: &T) -> Response {
    match serde_json::to_value(v) {
        Ok(p) => Response::ok(id, p),
        Err(e) => Response::err(id, code::INTERNAL, e.to_string()),
    }
}

fn internal(id: Option<String>, e: &odm_core::OdmError) -> Response {
    Response::err(id, code::INTERNAL, user_message(e))
}

/// Handles one request. Never panics; always echoes `req.id`.
pub async fn dispatch(ctx: &IpcContext, req: Request) -> Response {
    let id = req.id.clone();
    ctx.contact.record(&req.cmd);
    match req.cmd.as_str() {
        cmd::PING => reply(
            id,
            &PingReply {
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        ),
        cmd::GET_SUMMARY => get_summary(ctx, id).await,
        cmd::PAUSE_ALL => match ctx.manager.pause_all().await {
            Ok(()) => Response::ok(id, serde_json::json!({})),
            Err(e) => internal(id, &e),
        },
        cmd::RESUME_ALL => match ctx.manager.resume_all().await {
            Ok(()) => Response::ok(id, serde_json::json!({})),
            Err(e) => internal(id, &e),
        },
        cmd::SHOW_WINDOW => {
            (ctx.show_window)();
            Response::ok(id, serde_json::json!({}))
        }
        cmd::ADD_STREAM => add_stream(ctx, id, req.payload).await,
        cmd::ADD_PAGE => add_page(ctx, id, req.payload).await,
        cmd::ADD_DOWNLOAD => add_download(ctx, id, req.payload).await,
        cmd::ADD_TORRENT => add_torrent(ctx, id, req.payload).await,
        other => {
            let shown: String = other.chars().take(64).collect();
            Response::err(
                id,
                code::UNKNOWN_COMMAND,
                format!("Unknown command: {shown}"),
            )
        }
    }
}

async fn get_summary(ctx: &IpcContext, id: Option<String>) -> Response {
    let counts = match ctx.manager.state_counts().await {
        Ok(c) => c,
        Err(e) => return internal(id, &e),
    };
    let active = counts
        .iter()
        .filter(|(s, _)| s.is_active())
        .map(|(_, n)| *n)
        .sum();
    let queued = counts.get(&DownloadState::Queued).copied().unwrap_or(0);
    let total_speed_bps = ctx.speeds.lock().unwrap_or_else(|e| e.into_inner()).total();
    reply(
        id,
        &Summary {
            active,
            queued,
            total_speed_bps,
        },
    )
}

fn confirm_wanted(ctx: &IpcContext) -> bool {
    ctx.settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .confirm_browser_downloads
}

/// "Start new downloads: Paused": added paused, without the New download dialog.
fn paused_wanted(ctx: &IpcContext) -> bool {
    crate::api::starts_paused(&ctx.settings.lock().unwrap_or_else(|e| e.into_inner()))
}

/// The download was added paused: queue it for the "New download" dialog and bring the window up.
fn ask_to_confirm(ctx: &IpcContext, download_id: &str) {
    ctx.pending.push(download_id.to_string());
    (ctx.on_confirm)(download_id);
    (ctx.show_window)();
}

/// Always an explicit click in the browser, so the capture filters do not apply.
async fn add_stream(ctx: &IpcContext, id: Option<String>, payload: Value) -> Response {
    let p: AddStream = match serde_json::from_value(payload) {
        Ok(p) => p,
        Err(e) => return Response::err(id, code::BAD_PAYLOAD, e.to_string()),
    };
    tracing::info!(
        cmd = cmd::ADD_STREAM,
        url = %redact_url(&p.playlist_url),
        headers = ?p.headers.keys().collect::<Vec<_>>(),
        "ipc request"
    );
    let mut headers: Vec<(String, String)> = p
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if !p.page_url.is_empty()
        && !headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("referer"))
    {
        headers.push(("Referer".into(), p.page_url.clone()));
    }
    if let Some(c) = p.cookies.as_deref().filter(|c| !c.is_empty()) {
        headers.push(("Cookie".into(), c.to_string()));
    }
    let mut req = StreamRequest {
        playlist_url: p.playlist_url,
        page_url: Some(p.page_url).filter(|u| !u.is_empty()),
        page_title: p.page_title.filter(|t| !t.trim().is_empty()),
        headers: normalize_headers(headers),
        ..Default::default()
    };
    let confirm = confirm_wanted(ctx);
    req.start_paused = confirm || paused_wanted(ctx);
    match ctx.manager.add_stream(req).await {
        Ok(download_id) => {
            if confirm {
                ask_to_confirm(ctx, &download_id);
            }
            reply(id, &Added { download_id })
        }
        Err(e) => Response::err(id, code::ADD_FAILED, user_message(&e)),
    }
}

/// Always an explicit click in the browser. yt-dlp finds the media on the page.
async fn add_page(ctx: &IpcContext, id: Option<String>, payload: Value) -> Response {
    let p: AddPage = match serde_json::from_value(payload) {
        Ok(p) => p,
        Err(e) => return Response::err(id, code::BAD_PAYLOAD, e.to_string()),
    };
    tracing::info!(
        cmd = cmd::ADD_PAGE,
        url = %redact_url(&p.page_url),
        headers = ?p.headers.keys().collect::<Vec<_>>(),
        "ipc request"
    );
    let mut headers: Vec<(String, String)> = p
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if !headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("referer"))
    {
        headers.push(("Referer".into(), p.page_url.clone()));
    }
    if let Some(c) = p.cookies.as_deref().filter(|c| !c.is_empty()) {
        headers.push(("Cookie".into(), c.to_string()));
    }
    let confirm = confirm_wanted(ctx);
    let req = SiteRequest {
        page_url: p.page_url,
        page_title: p.page_title.filter(|t| !t.trim().is_empty()),
        headers: normalize_headers(headers),
        start_paused: confirm || paused_wanted(ctx),
        ..Default::default()
    };
    match ctx.manager.add_site(req).await {
        Ok(download_id) => {
            if confirm {
                ask_to_confirm(ctx, &download_id);
            }
            reply(id, &Added { download_id })
        }
        Err(e) => Response::err(id, code::ADD_FAILED, user_message(&e)),
    }
}

/// A magnet click or a `.torrent` download in the browser. Torrents always show the
/// confirm dialog, whatever the browser-confirm setting says.
async fn add_torrent(ctx: &IpcContext, id: Option<String>, payload: Value) -> Response {
    let p: AddTorrent = match serde_json::from_value(payload) {
        Ok(p) => p,
        Err(e) => return Response::err(id, code::BAD_PAYLOAD, e.to_string()),
    };
    tracing::info!(cmd = cmd::ADD_TORRENT, kind = ?p.kind, link = %redact_url(&p.link), "ipc request");
    let input = match p.kind {
        TorrentLinkKind::Magnet => crate::torrent::TorrentInput::Text(p.link),
        TorrentLinkKind::Url => {
            let mut headers: Vec<(String, String)> = p.headers.into_iter().collect();
            if let Some(r) = p.referrer.filter(|r| !r.is_empty()) {
                if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("referer")) {
                    headers.push(("Referer".into(), r));
                }
            }
            if let Some(c) = p.cookies.filter(|c| !c.is_empty()) {
                headers.push(("Cookie".into(), c));
            }
            crate::torrent::TorrentInput::Url {
                url: p.link,
                headers: normalize_headers(headers),
            }
        }
    };
    let http = {
        let cfg = ctx.settings.lock().unwrap_or_else(|e| e.into_inner()).to_engine_config();
        odm_engine::download::build_client(&cfg)
    };
    match crate::api::add_torrent(&ctx.manager, &ctx.pending, &http, input).await {
        Ok(added) => {
            if added.existing {
                (ctx.show_window)();
            } else {
                (ctx.on_confirm)(&added.id);
                (ctx.show_window)();
            }
            reply(id, &Added { download_id: added.id })
        }
        Err(e) => Response::err(id, code::ADD_FAILED, e),
    }
}

async fn add_download(ctx: &IpcContext, id: Option<String>, payload: Value) -> Response {
    let p: AddDownload = match serde_json::from_value(payload) {
        Ok(p) => p,
        Err(e) => return Response::err(id, code::BAD_PAYLOAD, e.to_string()),
    };
    tracing::info!(
        cmd = cmd::ADD_DOWNLOAD,
        url = %redact_url(&p.url),
        headers = ?p.headers.keys().collect::<Vec<_>>(),
        "ipc request"
    );
    if p.expires_at_ms.is_some_and(|t| now_ms() > t) {
        return Response::err(
            id,
            code::EXPIRED,
            "ODM answered too late; the browser kept the download.",
        );
    }
    if p.source == Source::Capture {
        let settings = ctx
            .settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(reason) = skip_reason(&settings, &p.url, p.filename.as_deref(), p.filesize) {
            return Response::err(id, code::SKIPPED, reason);
        }
    }

    let mut headers: Vec<(String, String)> = p
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if let Some(r) = p.referrer.as_deref().filter(|r| !r.is_empty()) {
        if !headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("referer"))
        {
            headers.push(("Referer".into(), r.to_string()));
        }
    }
    if let Some(c) = p.cookies.as_deref().filter(|c| !c.is_empty()) {
        headers.push(("Cookie".into(), c.to_string()));
    }
    let headers = normalize_headers(headers);

    let basename = p
        .filename
        .as_deref()
        .and_then(|f| f.rsplit(['/', '\\']).next())
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(str::to_string);
    let mut req = AddRequest {
        filename: basename.clone(),
        page_url: p.referrer.clone(),
        headers,
        size_hint: p.filesize,
        ..AddRequest::new(p.url.clone())
    };

    let name = match &basename {
        Some(b) => Some(filename::sanitize(b)),
        None => filename::from_url(&p.url),
    };
    if let Some(name) = name {
        match find_reusable(&ctx.manager, &name).await {
            Ok(Some(rec)) => {
                let offer = ctx.offers.create(rec.id.clone(), rec.filename.clone(), req);
                (ctx.on_offer)(&offer);
                (ctx.show_window)();
                return reply(
                    id,
                    &Added {
                        download_id: rec.id,
                    },
                );
            }
            Ok(None) => {}
            Err(e) => return internal(id, &e),
        }
    }

    let confirm = confirm_wanted(ctx);
    req.start_paused = confirm || paused_wanted(ctx);
    match ctx.manager.add(req).await {
        Ok(download_id) => {
            if confirm {
                ask_to_confirm(ctx, &download_id);
            }
            reply(id, &Added { download_id })
        }
        Err(e) => Response::err(id, code::ADD_FAILED, user_message(&e)),
    }
}
