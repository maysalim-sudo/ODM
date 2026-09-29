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

#[cfg(test)]
mod tests {
    #[test]
    fn pipe_name_format() {
        assert_eq!(super::pipe_name("Torinou"), r"\\.\pipe\odm-Torinou");
    }
}
