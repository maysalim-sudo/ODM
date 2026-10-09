//! Product identity. The ONLY place these values are defined.
pub const PRODUCT_NAME: &str = "ODM";
pub const BUNDLE_ID: &str = "com.odm.app";
pub const HOST_NAME: &str = "com.odm.host";
pub const FIREFOX_EXT_ID: &str = "odm@odm.local";
pub const APP_DIR_NAME: &str = "ODM";

/// The fixed Chromium extension ID (derived from `extension/chromium-key.txt`).
pub fn chromium_ext_id() -> &'static str {
    include_str!("../../../extension/chromium-id.txt").trim()
}

/// Extension IDs that may talk to the host: the dev ID plus the store-assigned IDs once known.
/// Source: `extension/chromium-ids.json`.
pub fn parse_chromium_ids(json: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    ["dev", "chrome_web_store", "edge_addons"]
        .iter()
        .filter_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .map(str::to_string)
        .collect()
}

/// Every Chromium extension ID the host must accept (dev ID first).
pub fn chromium_ext_ids() -> Vec<&'static str> {
    static IDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    IDS.get_or_init(|| parse_chromium_ids(include_str!("../../../extension/chromium-ids.json")))
        .iter()
        .map(String::as_str)
        .collect()
}

/// Where users can add the extension from a store. Null entries mean "not published yet".
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct StoreLinks {
    pub chrome_web_store_id: Option<String>,
    pub edge_addons_id: Option<String>,
    pub amo_url: Option<String>,
}

/// `ids_json` is `chromium-ids.json`; `links_json` is `store-links.json`.
pub fn parse_store_links(ids_json: &str, links_json: &str) -> StoreLinks {
    let get = |json: &str, key: &str| {
        serde_json::from_str::<serde_json::Value>(json)
            .ok()
            .and_then(|v| v.get(key).and_then(|x| x.as_str()).map(str::to_string))
    };
    StoreLinks {
        chrome_web_store_id: get(ids_json, "chrome_web_store"),
        edge_addons_id: get(ids_json, "edge_addons"),
        amo_url: get(links_json, "amo_url"),
    }
}

pub fn store_links() -> StoreLinks {
    parse_store_links(
        include_str!("../../../extension/chromium-ids.json"),
        include_str!("../../../extension/store-links.json"),
    )
}

/// Named pipe path for the given Windows user name.
pub fn pipe_name(user: &str) -> String {
    format!(r"\\.\pipe\odm-{user}")
}

/// The pipe name to use: the override when non-empty, else the per-user name.
pub fn pipe_name_for(override_name: Option<String>, user: Option<String>) -> String {
    match override_name {
        Some(n) if !n.is_empty() => n,
        _ => pipe_name(user.as_deref().unwrap_or("user")),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn pipe_name_override_wins_when_non_empty() {
        assert_eq!(
            super::pipe_name_for(Some(r"\\.\pipe\t1".into()), Some("u".into())),
            r"\\.\pipe\t1"
        );
        assert_eq!(
            super::pipe_name_for(Some(String::new()), Some("Torinou".into())),
            r"\\.\pipe\odm-Torinou"
        );
        assert_eq!(super::pipe_name_for(None, None), r"\\.\pipe\odm-user");
    }

    #[test]
    fn chromium_id_shape() {
        let id = super::chromium_ext_id();
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| ('a'..='p').contains(&c)));
    }

    #[test]
    fn parse_ids_keeps_only_non_null_entries() {
        let json = r#"{"dev":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","chrome_web_store":null,"edge_addons":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}"#;
        assert_eq!(
            super::parse_chromium_ids(json),
            vec![
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()
            ]
        );
        assert!(super::parse_chromium_ids("not json").is_empty());
    }

    #[test]
    fn shipped_ids_start_with_the_dev_id() {
        let ids = super::chromium_ext_ids();
        assert_eq!(ids[0], super::chromium_ext_id());
        assert!(ids.iter().all(|id| id.len() == 32));
    }

    #[test]
    fn parse_store_links_reads_ids_and_amo_url() {
        let l = super::parse_store_links(
            r#"{"dev":"d","chrome_web_store":"c","edge_addons":null}"#,
            r#"{"amo_url":"https://addons.mozilla.org/x"}"#,
        );
        assert_eq!(l.chrome_web_store_id.as_deref(), Some("c"));
        assert_eq!(l.edge_addons_id, None);
        assert_eq!(l.amo_url.as_deref(), Some("https://addons.mozilla.org/x"));
        assert_eq!(
            super::parse_store_links("junk", "junk"),
            super::StoreLinks::default()
        );
    }

    #[test]
    fn pipe_name_format() {
        assert_eq!(super::pipe_name("Torinou"), r"\\.\pipe\odm-Torinou");
    }
}
