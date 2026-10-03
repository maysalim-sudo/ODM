use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Timeout,
    Connection,
    Dns,
    /// 408, 429, 5xx
    HttpTransient(u16),
    /// Any other non-success status
    Http(u16),
    DiskFull,
    PermissionDenied,
    RangeIgnored,
    ChecksumMismatch,
    /// 403/410 after the download had already started
    LinkExpired,
    /// Validator (ETag/Last-Modified) no longer matches
    FileChanged,
    Io,
    Cancelled,
    Other,
}

impl ErrorKind {
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::Connection | Self::Dns | Self::HttpTransient(_)
        )
    }

    pub fn code(&self) -> String {
        match self {
            Self::HttpTransient(s) | Self::Http(s) => format!("http_{s}"),
            Self::Timeout => "timeout".into(),
            Self::Connection => "connection".into(),
            Self::Dns => "dns".into(),
            Self::DiskFull => "disk_full".into(),
            Self::PermissionDenied => "permission_denied".into(),
            Self::RangeIgnored => "range_ignored".into(),
            Self::ChecksumMismatch => "checksum_mismatch".into(),
            Self::LinkExpired => "link_expired".into(),
            Self::FileChanged => "file_changed".into(),
            Self::Io => "io".into(),
            Self::Cancelled => "cancelled".into(),
            Self::Other => "other".into(),
        }
    }

    /// Classifies an HTTP error status. 408, 429 and 5xx are transient; 403 and 410
    /// mean the link expired once the download had started.
    pub fn from_http_status(status: u16, started: bool) -> Self {
        if is_transient_status(status) {
            Self::HttpTransient(status)
        } else if matches!(status, 403 | 410) && started {
            Self::LinkExpired
        } else {
            Self::Http(status)
        }
    }

    /// The inverse of [`ErrorKind::code`]. `http_<n>` is transient for 408, 429 and 5xx.
    pub fn from_code(code: &str) -> Option<Self> {
        if let Some(n) = code.strip_prefix("http_") {
            let status: u16 = n.parse().ok()?;
            return Some(if is_transient_status(status) {
                Self::HttpTransient(status)
            } else {
                Self::Http(status)
            });
        }
        Some(match code {
            "timeout" => Self::Timeout,
            "connection" => Self::Connection,
            "dns" => Self::Dns,
            "disk_full" => Self::DiskFull,
            "permission_denied" => Self::PermissionDenied,
            "range_ignored" => Self::RangeIgnored,
            "checksum_mismatch" => Self::ChecksumMismatch,
            "link_expired" => Self::LinkExpired,
            "file_changed" => Self::FileChanged,
            "io" => Self::Io,
            "cancelled" => Self::Cancelled,
            "other" => Self::Other,
            _ => return None,
        })
    }
}

fn is_transient_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 500..=599)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdmError {
    pub kind: ErrorKind,
    pub message: String,
}

impl OdmError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for OdmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind.code(), self.message)
    }
}

impl std::error::Error for OdmError {}

#[cfg(test)]
mod tests {
    use super::ErrorKind::*;

    #[test]
    fn transient_classification() {
        for k in [
            Timeout,
            Connection,
            Dns,
            HttpTransient(503),
            HttpTransient(429),
        ] {
            assert!(k.is_transient(), "{k:?}");
        }
        for k in [
            Http(404),
            DiskFull,
            PermissionDenied,
            RangeIgnored,
            ChecksumMismatch,
            LinkExpired,
            FileChanged,
            Io,
            Cancelled,
            Other,
        ] {
            assert!(!k.is_transient(), "{k:?}");
        }
    }

    #[test]
    fn codes() {
        assert_eq!(Http(404).code(), "http_404");
        assert_eq!(HttpTransient(503).code(), "http_503");
        assert_eq!(LinkExpired.code(), "link_expired");
    }

    #[test]
    fn from_code_round_trips_every_variant() {
        for k in [
            Timeout,
            Connection,
            Dns,
            HttpTransient(408),
            HttpTransient(429),
            HttpTransient(503),
            Http(404),
            Http(403),
            DiskFull,
            PermissionDenied,
            RangeIgnored,
            ChecksumMismatch,
            LinkExpired,
            FileChanged,
            Io,
            Cancelled,
            Other,
        ] {
            assert_eq!(super::ErrorKind::from_code(&k.code()), Some(k), "{k:?}");
        }
        assert_eq!(super::ErrorKind::from_code("nonsense"), None);
        assert_eq!(super::ErrorKind::from_code("http_"), None);
        assert_eq!(super::ErrorKind::from_code("http_abc"), None);
    }

    #[test]
    fn from_http_status_cases() {
        let f = super::ErrorKind::from_http_status;
        assert_eq!(f(503, false), HttpTransient(503));
        assert_eq!(f(503, true), HttpTransient(503));
        assert_eq!(f(429, false), HttpTransient(429));
        assert_eq!(f(404, true), Http(404));
        assert_eq!(f(403, true), LinkExpired);
        assert_eq!(f(403, false), Http(403));
        assert_eq!(f(410, true), LinkExpired);
    }

    #[test]
    fn display_includes_message() {
        let e = super::OdmError::new(DiskFull, "no space");
        assert_eq!(e.to_string(), "disk_full: no space");
    }
}
