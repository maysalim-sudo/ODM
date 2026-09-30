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
    fn display_includes_message() {
        let e = super::OdmError::new(DiskFull, "no space");
        assert_eq!(e.to_string(), "disk_full: no space");
    }
}
