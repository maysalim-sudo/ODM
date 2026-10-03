use odm_core::{ErrorKind, OdmError};
use std::time::Duration;

const CAP_SECS: f64 = 60.0;
const RETRY_AFTER_CAP: u64 = 300;

pub fn backoff_with_jitter(attempt: u32, retry_after: Option<Duration>, jitter: f64) -> Duration {
    if let Some(ra) = retry_after {
        return ra.min(Duration::from_secs(RETRY_AFTER_CAP));
    }
    let exp = attempt.max(1).saturating_sub(1).min(10) as i32;
    let base = 2f64.powi(exp).min(CAP_SECS);
    let secs = (base * (1.0 + jitter.clamp(-0.2, 0.2)) * 1000.0).round() / 1000.0;
    Duration::from_secs_f64(secs)
}

pub fn backoff_delay(attempt: u32, retry_after: Option<Duration>) -> Duration {
    backoff_with_jitter(attempt, retry_after, fastrand::f64() * 0.4 - 0.2)
}

pub fn parse_retry_after(v: &str) -> Option<Duration> {
    v.trim()
        .parse::<u64>()
        .ok()
        .map(|s| Duration::from_secs(s.min(RETRY_AFTER_CAP)))
}

pub fn classify_status(status: u16, started: bool) -> ErrorKind {
    ErrorKind::from_http_status(status, started)
}

pub fn classify_io(e: &std::io::Error) -> ErrorKind {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::PermissionDenied => ErrorKind::PermissionDenied,
        K::StorageFull => ErrorKind::DiskFull,
        // 112 = ERROR_DISK_FULL, 39 = ERROR_HANDLE_DISK_FULL
        _ if matches!(e.raw_os_error(), Some(112) | Some(39)) => ErrorKind::DiskFull,
        _ => ErrorKind::Io,
    }
}

pub fn classify_reqwest(e: &reqwest::Error) -> ErrorKind {
    if e.is_timeout() {
        ErrorKind::Timeout
    } else if let Some(s) = e.status() {
        classify_status(s.as_u16(), false)
    } else if e.is_connect() || e.is_body() || e.is_request() || e.is_decode() {
        ErrorKind::Connection
    } else {
        ErrorKind::Other
    }
}

pub fn io_err(e: std::io::Error) -> OdmError {
    OdmError::new(classify_io(&e), e.to_string())
}

pub fn http_err(e: reqwest::Error) -> OdmError {
    let kind = classify_reqwest(&e);
    OdmError::new(kind, e.without_url().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use odm_core::ErrorKind::*;

    #[test]
    fn exponential_with_cap() {
        let d = |a| backoff_with_jitter(a, None, 0.0).as_secs_f64();
        assert_eq!([d(1), d(2), d(3), d(4)], [1.0, 2.0, 4.0, 8.0]);
        assert_eq!(d(7), 60.0);
        assert_eq!(d(30), 60.0);
    }

    #[test]
    fn jitter_bounds() {
        assert_eq!(backoff_with_jitter(2, None, 0.2).as_secs_f64(), 2.4);
        assert_eq!(backoff_with_jitter(2, None, -0.2).as_secs_f64(), 1.6);
        for _ in 0..100 {
            let s = backoff_delay(3, None).as_secs_f64();
            assert!((3.2..=4.8).contains(&s), "{s}");
        }
    }

    #[test]
    fn retry_after_wins_and_is_capped() {
        assert_eq!(
            backoff_with_jitter(1, Some(Duration::from_secs(7)), 0.2),
            Duration::from_secs(7)
        );
        assert_eq!(parse_retry_after("12"), Some(Duration::from_secs(12)));
        assert_eq!(parse_retry_after("100000"), Some(Duration::from_secs(300)));
        assert_eq!(parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT"), None);
    }

    #[test]
    fn status_classification() {
        assert_eq!(classify_status(503, false), HttpTransient(503));
        assert_eq!(classify_status(429, true), HttpTransient(429));
        assert_eq!(classify_status(408, false), HttpTransient(408));
        assert_eq!(classify_status(404, true), Http(404));
        assert_eq!(classify_status(403, false), Http(403));
        assert_eq!(classify_status(403, true), LinkExpired);
        assert_eq!(classify_status(410, true), LinkExpired);
    }

    #[test]
    fn io_classification() {
        use std::io::{Error, ErrorKind as K};
        assert_eq!(
            classify_io(&Error::from(K::PermissionDenied)),
            PermissionDenied
        );
        assert_eq!(classify_io(&Error::from(K::StorageFull)), DiskFull);
        assert_eq!(classify_io(&Error::from_raw_os_error(112)), DiskFull); // ERROR_DISK_FULL on Windows
        assert_eq!(classify_io(&Error::from(K::NotFound)), Io);
    }
}
