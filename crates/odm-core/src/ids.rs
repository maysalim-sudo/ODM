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

/// The browser helper's file name, next to the app: `odm-host.exe` on Windows, `odm-host` elsewhere.
pub fn host_exe_name() -> String {
    format!("odm-host{}", std::env::consts::EXE_SUFFIX)
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

/// Unix socket path: `<runtime dir>/odm.sock` when the runtime dir is set and absolute, else
/// `/tmp/odm-<uid>.sock`.
pub fn socket_path(runtime_dir: Option<&std::path::Path>, uid: u32) -> std::path::PathBuf {
    match runtime_dir {
        Some(d) if d.is_absolute() => d.join("odm.sock"),
        _ => std::path::PathBuf::from(format!("/tmp/odm-{uid}.sock")),
    }
}

/// The pipe (Windows) or socket (Unix) ODM listens on: `ODM_PIPE_NAME` when set and non-empty,
/// else `\\.\pipe\odm-<USERNAME>` on Windows and [`socket_path`] on Unix.
pub fn pipe_name_from_env() -> String {
    #[cfg(windows)]
    {
        pipe_name_for(
            std::env::var("ODM_PIPE_NAME").ok(),
            std::env::var("USERNAME").ok(),
        )
    }
    #[cfg(unix)]
    {
        match std::env::var("ODM_PIPE_NAME") {
            Ok(n) if !n.is_empty() => n,
            _ => {
                let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from);
                // SAFETY: getuid has no preconditions and cannot fail.
                let uid = unsafe { libc::getuid() };
                socket_path(runtime.as_deref(), uid)
                    .to_string_lossy()
                    .into_owned()
            }
        }
    }
}

/// Which manifest a native-messaging folder takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestKind {
    /// `allowed_origins` with the Chromium extension IDs.
    Chromium,
    /// `allowed_extensions` with the Firefox add-on ID.
    Mozilla,
}

/// Linux: each browser's user-level native-messaging folder, relative to `$HOME`.
/// The manifest in each is [`host_manifest_file`].
pub const LINUX_HOST_DIRS: [(&str, &str, ManifestKind); 8] = [
    (
        "Chrome",
        ".config/google-chrome/NativeMessagingHosts",
        ManifestKind::Chromium,
    ),
    (
        "Edge",
        ".config/microsoft-edge/NativeMessagingHosts",
        ManifestKind::Chromium,
    ),
    (
        "Brave",
        ".config/BraveSoftware/Brave-Browser/NativeMessagingHosts",
        ManifestKind::Chromium,
    ),
    (
        "Vivaldi",
        ".config/vivaldi/NativeMessagingHosts",
        ManifestKind::Chromium,
    ),
    (
        "Opera",
        ".config/opera/NativeMessagingHosts",
        ManifestKind::Chromium,
    ),
    (
        "Chromium",
        ".config/chromium/NativeMessagingHosts",
        ManifestKind::Chromium,
    ),
    (
        "Firefox",
        ".mozilla/native-messaging-hosts",
        ManifestKind::Mozilla,
    ),
    (
        "Floorp",
        ".floorp/native-messaging-hosts",
        ManifestKind::Mozilla,
    ),
];

/// `com.odm.host.json`: browsers on Linux find the host by this file name.
pub fn host_manifest_file() -> String {
    format!("{HOST_NAME}.json")
}

/// Linux stand-in for `HKCU\Software\ODM`: `{"HostPath": …, "AppPath": …}`.
pub fn registration_file(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".config")
        .join(APP_DIR_NAME)
        .join("registration.json")
}

/// One value from the registration file, or None if the file or the value is missing.
pub fn read_registration(home: &std::path::Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(registration_file(home)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get(key)?.as_str().map(str::to_string)
}

/// `$HOME` when set and not empty.
pub fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(std::path::PathBuf::from)
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

    // Unix only: Windows never uses the socket, and "/run/user/1000" is not absolute there.
    #[cfg(unix)]
    #[test]
    fn socket_path_uses_the_runtime_dir_when_absolute() {
        use std::path::{Path, PathBuf};
        assert_eq!(
            super::socket_path(Some(Path::new("/run/user/1000")), 1000),
            PathBuf::from("/run/user/1000/odm.sock")
        );
        assert_eq!(
            super::socket_path(Some(Path::new("relative")), 7),
            PathBuf::from("/tmp/odm-7.sock")
        );
        assert_eq!(
            super::socket_path(None, 7),
            PathBuf::from("/tmp/odm-7.sock")
        );
    }

    #[test]
    fn linux_host_dirs_cover_every_browser_once() {
        use super::ManifestKind::Mozilla;
        let names: Vec<_> = super::LINUX_HOST_DIRS.iter().map(|(n, _, _)| *n).collect();
        assert_eq!(
            names,
            ["Chrome", "Edge", "Brave", "Vivaldi", "Opera", "Chromium", "Firefox", "Floorp"]
        );
        for (name, dir, kind) in super::LINUX_HOST_DIRS {
            assert!(!dir.starts_with('/'), "{name}");
            let mozilla = dir.ends_with("native-messaging-hosts");
            assert_eq!(kind == Mozilla, mozilla, "{name}");
            assert!(mozilla || dir.ends_with("NativeMessagingHosts"), "{name}");
        }
        assert_eq!(super::host_manifest_file(), "com.odm.host.json");
    }

    #[test]
    fn registration_file_round_trip() {
        let home = std::env::temp_dir().join(format!("odm-core-reg-{}", std::process::id()));
        let file = super::registration_file(&home);
        assert!(file.ends_with(".config/ODM/registration.json"));
        assert_eq!(super::read_registration(&home, "AppPath"), None);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, r#"{"AppPath":"/usr/bin/odm-app","HostPath":7}"#).unwrap();
        assert_eq!(
            super::read_registration(&home, "AppPath").as_deref(),
            Some("/usr/bin/odm-app")
        );
        assert_eq!(super::read_registration(&home, "HostPath"), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn pipe_name_format() {
        assert_eq!(super::pipe_name("Torinou"), r"\\.\pipe\odm-Torinou");
    }
}
