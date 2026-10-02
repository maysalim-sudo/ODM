//! Make a URL safe to write to a log.

/// Remove the query string, the fragment, and any `user:pass@` credentials.
pub fn redact_url(url: &str) -> String {
    let no_tail = url.split(['?', '#']).next().unwrap_or(url);
    let Some((scheme, rest)) = no_tail.split_once("://") else {
        return no_tail.to_string();
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    format!("{scheme}://{host}{path}")
}

#[cfg(test)]
mod tests {
    use super::redact_url;

    #[test]
    fn strips_query_fragment_and_userinfo() {
        assert_eq!(
            redact_url("https://a.com/f.zip?token=secret#x"),
            "https://a.com/f.zip"
        );
        assert_eq!(
            redact_url("https://user:pw@a.com/f.zip"),
            "https://a.com/f.zip"
        );
        assert_eq!(
            redact_url("https://user@a.com:8443/p/q?x=1"),
            "https://a.com:8443/p/q"
        );
        assert_eq!(redact_url("http://a.com"), "http://a.com");
    }

    #[test]
    fn an_at_sign_in_the_path_is_not_userinfo() {
        assert_eq!(
            redact_url("https://a.com/files/me@home.txt"),
            "https://a.com/files/me@home.txt"
        );
    }

    #[test]
    fn non_urls_lose_only_query_and_fragment() {
        assert_eq!(redact_url("not a url?x=1"), "not a url");
    }
}
