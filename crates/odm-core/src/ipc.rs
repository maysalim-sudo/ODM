//! Wire protocol between the browser extension, the native host and the app.
//!
//! Frames are a 4-byte little-endian length followed by UTF-8 JSON, at most
//! [`MAX_FRAME`] bytes, on both the stdio and the pipe link.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};

pub const MAX_FRAME: usize = 1 << 20;

pub mod cmd {
    pub const PING: &str = "PING";
    pub const ADD_DOWNLOAD: &str = "ADD_DOWNLOAD";
    pub const ADD_STREAM: &str = "ADD_STREAM";
    pub const ADD_PAGE: &str = "ADD_PAGE";
    pub const GET_SUMMARY: &str = "GET_SUMMARY";
    pub const PAUSE_ALL: &str = "PAUSE_ALL";
    pub const RESUME_ALL: &str = "RESUME_ALL";
    pub const SHOW_WINDOW: &str = "SHOW_WINDOW";
}

pub mod code {
    pub const BAD_MESSAGE: &str = "bad_message";
    pub const TOO_LARGE: &str = "too_large";
    pub const UNKNOWN_COMMAND: &str = "unknown_command";
    pub const BAD_PAYLOAD: &str = "bad_payload";
    pub const SKIPPED: &str = "skipped";
    pub const EXPIRED: &str = "expired";
    pub const ADD_FAILED: &str = "add_failed";
    pub const NOT_SUPPORTED: &str = "not_supported";
    pub const APP_NOT_RUNNING: &str = "app_not_running";
    pub const APP_START_FAILED: &str = "app_start_failed";
    pub const PIPE_ERROR: &str = "pipe_error";
    pub const TIMEOUT: &str = "timeout";
    pub const INTERNAL: &str = "internal";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub id: Option<String>,
    pub cmd: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub id: Option<String>,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<IpcError>,
}

impl Response {
    pub fn ok(id: Option<String>, payload: serde_json::Value) -> Self {
        Self {
            id,
            ok: true,
            payload: Some(payload),
            error: None,
        }
    }

    pub fn err(id: Option<String>, code: &str, message: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            payload: None,
            error: Some(IpcError {
                code: code.to_string(),
                message: message.into(),
            }),
        }
    }

    /// Serialises the reply. Serialising these types cannot fail.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_else(|_| {
            br#"{"id":null,"ok":false,"error":{"code":"internal","message":"serialise failed"}}"#
                .to_vec()
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Capture,
    Menu,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AddDownload {
    pub url: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub referrer: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub cookies: Option<String>,
    #[serde(default)]
    pub filesize: Option<u64>,
    #[serde(default)]
    pub mime: Option<String>,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub expires_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AddStream {
    pub page_url: String,
    #[serde(default)]
    pub page_title: Option<String>,
    pub playlist_url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub cookies: Option<String>,
}

/// A site page (YouTube, Facebook…) whose video yt-dlp downloads.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AddPage {
    pub page_url: String,
    #[serde(default)]
    pub page_title: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub cookies: Option<String>,
}

#[derive(Serialize)]
pub struct PingReply {
    pub version: String,
}

#[derive(Serialize)]
pub struct Summary {
    pub active: u64,
    pub queued: u64,
    pub total_speed_bps: u64,
}

#[derive(Serialize)]
pub struct Added {
    pub download_id: String,
}

/// Commands for which the host may start ODM when the pipe is missing.
pub fn starts_app(cmd: &str) -> bool {
    matches!(
        cmd,
        cmd::ADD_DOWNLOAD | cmd::ADD_STREAM | cmd::ADD_PAGE | cmd::SHOW_WINDOW
    )
}

#[derive(Debug)]
pub enum FrameError {
    Eof,
    TooLarge(usize),
    Truncated,
    Io(std::io::Error),
}

pub fn frame_len(header: [u8; 4]) -> Result<usize, FrameError> {
    let n = u32::from_le_bytes(header) as usize;
    if n > MAX_FRAME {
        Err(FrameError::TooLarge(n))
    } else {
        Ok(n)
    }
}

/// Fills `buf` as far as the stream allows and returns how many bytes were read.
fn read_up_to<R: Read>(r: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match r.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(got)
}

/// `Eof` only when the stream ends before the first header byte. On
/// `TooLarge(n)` only the 4 header bytes were consumed; the caller may
/// `skip_bytes(n)` to stay in sync.
pub fn read_frame<R: Read>(r: &mut R) -> Result<Vec<u8>, FrameError> {
    let mut header = [0u8; 4];
    match read_up_to(r, &mut header).map_err(FrameError::Io)? {
        0 => return Err(FrameError::Eof),
        4 => {}
        _ => return Err(FrameError::Truncated),
    }
    let n = frame_len(header)?;
    let mut body = vec![0u8; n];
    if read_up_to(r, &mut body).map_err(FrameError::Io)? < n {
        return Err(FrameError::Truncated);
    }
    Ok(body)
}

pub fn write_frame<W: Write>(w: &mut W, body: &[u8]) -> std::io::Result<()> {
    if body.len() > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "frame exceeds 1 MiB",
        ));
    }
    w.write_all(&(body.len() as u32).to_le_bytes())?;
    w.write_all(body)?;
    w.flush()
}

pub fn skip_bytes<R: Read>(r: &mut R, n: usize) -> std::io::Result<()> {
    let copied = std::io::copy(&mut r.take(n as u64), &mut std::io::sink())?;
    if copied < n as u64 {
        return Err(std::io::ErrorKind::UnexpectedEof.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, br#"{"cmd":"PING"}"#).unwrap();
        assert_eq!(&buf[..4], &14u32.to_le_bytes());
        assert_eq!(read_frame(&mut &buf[..]).unwrap(), br#"{"cmd":"PING"}"#);
    }

    #[test]
    fn clean_eof_vs_truncated() {
        assert!(matches!(read_frame(&mut &b""[..]), Err(FrameError::Eof)));
        assert!(matches!(
            read_frame(&mut &[5u8, 0][..]),
            Err(FrameError::Truncated)
        ));
        assert!(matches!(
            read_frame(&mut &[5u8, 0, 0, 0, b'{'][..]),
            Err(FrameError::Truncated)
        ));
    }

    #[test]
    fn oversize_frame_is_reported_and_skippable() {
        let n = MAX_FRAME + 1;
        let mut data = (n as u32).to_le_bytes().to_vec();
        data.extend(std::iter::repeat_n(b' ', n));
        write_frame(&mut data, b"{}").unwrap(); // a valid frame follows
        let mut r = &data[..];
        match read_frame(&mut r) {
            Err(FrameError::TooLarge(m)) => {
                assert_eq!(m, n);
                skip_bytes(&mut r, m).unwrap();
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(read_frame(&mut r).unwrap(), b"{}");
    }

    #[test]
    fn exact_limit_is_accepted() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &vec![b' '; MAX_FRAME]).unwrap();
        assert_eq!(read_frame(&mut &buf[..]).unwrap().len(), MAX_FRAME);
        assert!(write_frame(&mut Vec::new(), &vec![b' '; MAX_FRAME + 1]).is_err());
    }

    #[test]
    fn request_tolerates_missing_id_and_payload() {
        let r: Request = serde_json::from_str(r#"{"cmd":"PING"}"#).unwrap();
        assert_eq!(
            (r.id, r.cmd.as_str(), r.payload),
            (None, "PING", serde_json::Value::Null)
        );
    }

    #[test]
    fn response_shapes() {
        let ok = serde_json::to_value(Response::ok(
            Some("1".into()),
            serde_json::json!({"version":"0.1.0"}),
        ))
        .unwrap();
        assert_eq!(
            ok,
            serde_json::json!({"id":"1","ok":true,"payload":{"version":"0.1.0"}})
        );
        let e = serde_json::to_value(Response::err(None, code::SKIPPED, "capture is off")).unwrap();
        assert_eq!(
            e,
            serde_json::json!({"id":null,"ok":false,"error":{"code":"skipped","message":"capture is off"}})
        );
    }

    #[test]
    fn add_download_defaults() {
        let a: AddDownload = serde_json::from_str(r#"{"url":"https://x/a.zip"}"#).unwrap();
        assert_eq!(
            (a.source, a.headers.len(), a.filesize, a.expires_at_ms),
            (Source::Capture, 0, None, None)
        );
        let m: AddDownload =
            serde_json::from_str(r#"{"url":"u","source":"menu","filesize":5}"#).unwrap();
        assert_eq!((m.source, m.filesize), (Source::Menu, Some(5)));
        assert!(serde_json::from_value::<AddDownload>(serde_json::json!({"url": 5})).is_err());
    }

    #[test]
    fn only_add_and_show_start_the_app() {
        for c in [
            cmd::ADD_DOWNLOAD,
            cmd::ADD_STREAM,
            cmd::ADD_PAGE,
            cmd::SHOW_WINDOW,
        ] {
            assert!(starts_app(c));
        }
        for c in [
            cmd::PING,
            cmd::GET_SUMMARY,
            cmd::PAUSE_ALL,
            cmd::RESUME_ALL,
            "NOPE",
        ] {
            assert!(!starts_app(c));
        }
    }
}
