//! Capture filters: skip-hosts, header normalisation and the "keep it in the browser" decision.
use crate::Settings;
use reqwest::header::HeaderName;
use reqwest::Url;

/// Lower-case host without scheme, userinfo, port, path, trailing dot or a leading "www.". None if empty/invalid.
pub fn normalize_host(entry: &str) -> Option<String> {
    let mut s = entry.trim();
    if let Some((_, rest)) = s.split_once("://") {
        s = rest;
    }
    let s = s.split(['/', '?', '#']).next().unwrap_or("");
    let s = s.rsplit('@').next().unwrap_or("");
    let s = s.split(':').next().unwrap_or("");
    let s = s.to_ascii_lowercase();
    let s = s.trim_matches('.');
    let s = s.strip_prefix("www.").unwrap_or(s);
    if s.is_empty()
        || !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
    {
        return None;
    }
    Some(s.to_string())
}

/// True if the URL's host equals an entry or is a subdomain of one. Entries are already normalised.
pub fn host_skipped(url: &str, skip_hosts: &[String]) -> bool {
    let Ok(u) = Url::parse(url) else {
        return false;
    };
    let Some(host) = u.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    let host = host.trim_end_matches('.');
    skip_hosts
        .iter()
        .any(|e| host == e || host.ends_with(&format!(".{e}")))
}

const DROPPED: [&str; 12] = [
    "host",
    "connection",
    "keep-alive",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "content-length",
    "range",
    "if-range",
    "accept-encoding",
];

/// Trim names and values; drop invalid header names, values containing CR or LF, and hop-by-hop or
/// framing names. Case-insensitive duplicates: the last one wins, at the position of the first.
pub fn normalize_headers(h: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (name, value) in h {
        let (name, value) = (name.trim(), value.trim());
        if name.is_empty()
            || HeaderName::from_bytes(name.as_bytes()).is_err()
            || value.contains(['\r', '\n'])
            || DROPPED.iter().any(|d| name.eq_ignore_ascii_case(d))
        {
            continue;
        }
        match out.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some(slot) => *slot = (name.to_string(), value.to_string()),
            None => out.push((name.to_string(), value.to_string())),
        }
    }
    out
}

/// Why a captured download should stay in the browser, or None to take it.
pub fn skip_reason(s: &Settings, url: &str, filesize: Option<u64>) -> Option<&'static str> {
    if !s.capture_enabled {
        return Some("capture is off");
    }
    if filesize.is_some_and(|n| n < s.min_capture_bytes) {
        return Some("the file is smaller than the minimum capture size");
    }
    if host_skipped(url, &s.skip_hosts) {
        return Some("this site is on the skip list");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_hosts_match_loosely_typed_entries() {
        let list: Vec<String> = ["https://WWW.Example.com:8080/x", "cdn.Test.org.", "  "]
            .iter()
            .filter_map(|e| normalize_host(e))
            .collect();
        assert_eq!(list, ["example.com", "cdn.test.org"]);
        assert!(host_skipped("https://example.com/a.zip", &list));
        assert!(host_skipped("https://dl.EXAMPLE.com/a.zip", &list));
        assert!(host_skipped("http://www.example.com/a", &list));
        assert!(!host_skipped("https://notexample.com/a", &list));
        assert!(!host_skipped("https://test.org/a", &list));
        assert!(!host_skipped("not a url", &list));
    }

    #[test]
    fn normalize_headers_dedupes_and_drops_unsafe() {
        let s = |a: &str, b: &str| (a.to_string(), b.to_string());
        let got = normalize_headers(vec![
            s("Referer", "a"),
            s("cookie", "x=1"),
            s("Host", "h"),
            s("Cookie", "x=2"),
            s("Bad Name", "v"),
            s("X-Ok", "line\r\nInjected: 1"),
            s("User-Agent", " ua "),
        ]);
        assert_eq!(
            got,
            vec![s("Referer", "a"), s("Cookie", "x=2"), s("User-Agent", "ua")]
        );
    }

    #[test]
    fn skip_reason_checks_in_order() {
        let mut s = Settings::default();
        let url = "https://example.com/a.zip";
        assert_eq!(skip_reason(&s, url, Some(5 << 20)), None);
        assert_eq!(skip_reason(&s, url, None), None);
        assert_eq!(
            skip_reason(&s, url, Some(10)),
            Some("the file is smaller than the minimum capture size")
        );
        s.skip_hosts = vec!["example.com".into()];
        assert_eq!(
            skip_reason(&s, url, Some(5 << 20)),
            Some("this site is on the skip list")
        );
        s.capture_enabled = false;
        assert_eq!(skip_reason(&s, url, Some(5 << 20)), Some("capture is off"));
    }
}
