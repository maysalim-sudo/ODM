//! Registers the native messaging host with Chromium browsers, Firefox and Floorp: HKCU keys on
//! Windows, a manifest file in each browser's folder on Linux.
use serde_json::{json, Value};
use std::io;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use winreg::enums::HKEY_CURRENT_USER;
#[cfg(windows)]
use winreg::RegKey;

pub struct RegisterOptions {
    pub host_exe: PathBuf,
    pub app_exe: PathBuf,
    /// Every extension ID allowed to connect (dev + store IDs).
    pub chromium_ids: Vec<String>,
    /// Prepended to every HKCU subkey path. "" in production; "Software\ODM-test-<uuid>\\" in tests.
    #[cfg(windows)]
    pub hkcu_prefix: String,
    /// Where the browser folders and the registration file are. `$HOME` in production.
    #[cfg(unix)]
    pub home: PathBuf,
}

#[cfg(windows)]
pub const CHROMIUM_KEYS: [&str; 6] = [
    r"Software\Google\Chrome\NativeMessagingHosts",
    r"Software\Microsoft\Edge\NativeMessagingHosts",
    r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts",
    r"Software\Vivaldi\NativeMessagingHosts",
    r"Software\Opera Software\NativeMessagingHosts",
    r"Software\Chromium\NativeMessagingHosts",
];
/// Firefox and Floorp.
#[cfg(windows)]
pub const MOZILLA_KEY: &str = r"Software\Mozilla\NativeMessagingHosts";
#[cfg(windows)]
pub const ODM_KEY: &str = r"Software\ODM";

#[cfg(windows)]
const CHROMIUM_MANIFEST_FILE: &str = "com.odm.host.chromium.json";
#[cfg(windows)]
const FIREFOX_MANIFEST_FILE: &str = "com.odm.host.firefox.json";

fn base_manifest(host_exe: &Path) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    m.insert("name".into(), json!(odm_core::ids::HOST_NAME));
    m.insert("description".into(), json!("ODM native messaging host"));
    m.insert("path".into(), json!(host_exe.to_string_lossy()));
    m.insert("type".into(), json!("stdio"));
    m
}

pub fn chromium_manifest<S: AsRef<str>>(host_exe: &Path, chromium_ids: &[S]) -> Value {
    let mut m = base_manifest(host_exe);
    let origins: Vec<String> = chromium_ids
        .iter()
        .map(|id| format!("chrome-extension://{}/", id.as_ref()))
        .collect();
    m.insert("allowed_origins".into(), json!(origins));
    Value::Object(m)
}

pub fn firefox_manifest(host_exe: &Path) -> Value {
    let mut m = base_manifest(host_exe);
    m.insert(
        "allowed_extensions".into(),
        json!([odm_core::ids::FIREFOX_EXT_ID]),
    );
    Value::Object(m)
}

#[cfg(windows)]
fn host_dir(o: &RegisterOptions) -> io::Result<&Path> {
    o.host_exe
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "host path has no directory"))
}

#[cfg(windows)]
fn set_default(hkcu: &RegKey, path: &str, value: &Path) -> io::Result<()> {
    let (key, _) = hkcu.create_subkey(path)?;
    key.set_value("", &value.to_string_lossy().as_ref())
}

#[cfg(windows)]
pub fn register(o: &RegisterOptions) -> io::Result<()> {
    let dir = host_dir(o)?;
    let chromium_path = dir.join(CHROMIUM_MANIFEST_FILE);
    let firefox_path = dir.join(FIREFOX_MANIFEST_FILE);
    let pretty = |v: &Value| serde_json::to_vec_pretty(v).map_err(io::Error::other);
    std::fs::write(
        &chromium_path,
        pretty(&chromium_manifest(&o.host_exe, &o.chromium_ids))?,
    )?;
    std::fs::write(&firefox_path, pretty(&firefox_manifest(&o.host_exe))?)?;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let host = odm_core::ids::HOST_NAME;
    for key in CHROMIUM_KEYS {
        set_default(
            &hkcu,
            &format!(r"{}{key}\{host}", o.hkcu_prefix),
            &chromium_path,
        )?;
    }
    set_default(
        &hkcu,
        &format!(r"{}{MOZILLA_KEY}\{host}", o.hkcu_prefix),
        &firefox_path,
    )?;
    let (odm, _) = hkcu.create_subkey(format!("{}{ODM_KEY}", o.hkcu_prefix))?;
    odm.set_value("HostPath", &o.host_exe.to_string_lossy().as_ref())?;
    odm.set_value("AppPath", &o.app_exe.to_string_lossy().as_ref())?;
    Ok(())
}

fn ignore_missing(r: io::Result<()>) -> io::Result<()> {
    match r {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Missing keys and files are not errors. Parent keys are never deleted.
#[cfg(windows)]
pub fn unregister(o: &RegisterOptions) -> io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let host = odm_core::ids::HOST_NAME;
    for key in CHROMIUM_KEYS.iter().chain([&MOZILLA_KEY]) {
        ignore_missing(hkcu.delete_subkey_all(format!(r"{}{key}\{host}", o.hkcu_prefix)))?;
    }
    ignore_missing(hkcu.delete_subkey_all(format!("{}{ODM_KEY}", o.hkcu_prefix)))?;
    if let Ok(dir) = host_dir(o) {
        for f in [CHROMIUM_MANIFEST_FILE, FIREFOX_MANIFEST_FILE] {
            ignore_missing(std::fs::remove_file(dir.join(f)))?;
        }
    }
    Ok(())
}

/// The browser folder each native-messaging folder belongs to, e.g. `.config/google-chrome`.
#[cfg(unix)]
fn browser_dir(host_dir: &str) -> &Path {
    Path::new(host_dir).parent().unwrap_or(Path::new(host_dir))
}

/// Writes the manifest into the folder of every browser that is installed (its own folder
/// exists), then the registration file. Browsers installed later get theirs the next time ODM
/// starts, since ODM registers at every start on Linux.
#[cfg(unix)]
pub fn register(o: &RegisterOptions) -> io::Result<()> {
    use odm_core::ids::{host_manifest_file, registration_file, ManifestKind, LINUX_HOST_DIRS};
    let pretty = |v: &Value| serde_json::to_vec_pretty(v).map_err(io::Error::other);
    for (_, dir, kind) in LINUX_HOST_DIRS {
        if !o.home.join(browser_dir(dir)).is_dir() {
            continue;
        }
        let dir = o.home.join(dir);
        std::fs::create_dir_all(&dir)?;
        let manifest = match kind {
            ManifestKind::Chromium => chromium_manifest(&o.host_exe, &o.chromium_ids),
            ManifestKind::Mozilla => firefox_manifest(&o.host_exe),
        };
        std::fs::write(dir.join(host_manifest_file()), pretty(&manifest)?)?;
    }
    let file = registration_file(&o.home);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let record = json!({
        "HostPath": o.host_exe.to_string_lossy(),
        "AppPath": o.app_exe.to_string_lossy(),
    });
    std::fs::write(file, pretty(&record)?)
}

/// Missing files are not errors. Folders are never deleted.
#[cfg(unix)]
pub fn unregister(o: &RegisterOptions) -> io::Result<()> {
    use odm_core::ids::{host_manifest_file, registration_file, LINUX_HOST_DIRS};
    for (_, dir, _) in LINUX_HOST_DIRS {
        ignore_missing(std::fs::remove_file(
            o.home.join(dir).join(host_manifest_file()),
        ))?;
    }
    ignore_missing(std::fs::remove_file(registration_file(&o.home)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn manifests_match_spec() {
        let host = Path::new(r"C:\x\odm-host.exe");
        let id = "abcdefghijklmnopabcdefghijklmnop";
        assert_eq!(
            chromium_manifest(host, &[id]),
            json!({
                "name": "com.odm.host",
                "description": "ODM native messaging host",
                "path": r"C:\x\odm-host.exe",
                "type": "stdio",
                "allowed_origins": ["chrome-extension://abcdefghijklmnopabcdefghijklmnop/"]
            })
        );
        assert_eq!(
            firefox_manifest(host),
            json!({
                "name": "com.odm.host",
                "description": "ODM native messaging host",
                "path": r"C:\x\odm-host.exe",
                "type": "stdio",
                "allowed_extensions": ["odm@odm.local"]
            })
        );
    }

    #[test]
    fn chromium_manifest_lists_every_id() {
        let host = Path::new(r"C:xodm-host.exe");
        let ids = [
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "cccccccccccccccccccccccccccccccc",
        ];
        let m = chromium_manifest(host, &ids);
        assert_eq!(
            m["allowed_origins"],
            json!([
                "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/",
                "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/",
                "chrome-extension://cccccccccccccccccccccccccccccccc/"
            ])
        );
    }
}
