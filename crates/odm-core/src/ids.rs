//! Product identity. The ONLY place these values are defined.
pub const PRODUCT_NAME: &str = "ODM";
pub const BUNDLE_ID: &str = "com.odm.app";
pub const HOST_NAME: &str = "com.odm.host";
pub const FIREFOX_EXT_ID: &str = "odm@odm.local";
pub const APP_DIR_NAME: &str = "ODM";

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
    fn pipe_name_format() {
        assert_eq!(super::pipe_name("Torinou"), r"\\.\pipe\odm-Torinou");
    }
}
