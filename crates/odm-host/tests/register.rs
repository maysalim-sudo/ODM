//! Windows: registry round trip. Linux: see `register_unix.rs`.
#![cfg(windows)]

use odm_host::register::{
    register, unregister, RegisterOptions, CHROMIUM_KEYS, MOZILLA_KEY, ODM_KEY,
};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

struct Cleanup(String);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.0);
    }
}

fn default_of(prefix: &str, key: &str) -> Option<String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(format!(r"{prefix}{key}\com.odm.host"))
        .ok()?
        .get_value("")
        .ok()
}

#[test]
fn register_then_unregister_round_trip() {
    let id = uuid::Uuid::new_v4();
    let root = format!(r"Software\ODM-test-{id}");
    let _guard = Cleanup(root.clone());
    let prefix = format!(r"{root}\");
    let tmp = std::env::temp_dir().join(format!("odm-host-reg-{id}"));
    std::fs::create_dir_all(&tmp).unwrap();
    let host_exe = tmp.join("odm-host.exe");
    let app_exe = tmp.join("odm.exe");
    let o = RegisterOptions {
        host_exe: host_exe.clone(),
        app_exe: app_exe.clone(),
        chromium_ids: vec![
            "abcdefghijklmnopabcdefghijklmnop".into(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        ],
        hkcu_prefix: prefix.clone(),
    };

    register(&o).unwrap();
    let cm = tmp.join("com.odm.host.chromium.json");
    let fm = tmp.join("com.odm.host.firefox.json");
    let c: serde_json::Value = serde_json::from_slice(&std::fs::read(&cm).unwrap()).unwrap();
    let f: serde_json::Value = serde_json::from_slice(&std::fs::read(&fm).unwrap()).unwrap();
    assert_eq!(c["allowed_origins"].as_array().unwrap().len(), 2);
    assert!(f["allowed_extensions"].is_array());
    for k in CHROMIUM_KEYS {
        assert_eq!(
            default_of(&prefix, k),
            Some(cm.to_string_lossy().into_owned()),
            "{k}"
        );
    }
    assert_eq!(
        default_of(&prefix, MOZILLA_KEY),
        Some(fm.to_string_lossy().into_owned())
    );
    let odm = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(format!("{prefix}{ODM_KEY}"))
        .unwrap();
    assert_eq!(
        odm.get_value::<String, _>("HostPath").unwrap(),
        host_exe.to_string_lossy()
    );
    assert_eq!(
        odm.get_value::<String, _>("AppPath").unwrap(),
        app_exe.to_string_lossy()
    );

    unregister(&o).unwrap();
    assert!(!cm.exists() && !fm.exists());
    for k in CHROMIUM_KEYS.iter().chain([&MOZILLA_KEY]) {
        assert_eq!(default_of(&prefix, k), None, "{k}");
        let parent = RegKey::predef(HKEY_CURRENT_USER).open_subkey(format!("{prefix}{k}"));
        assert!(parent.is_ok(), "parent {k} must remain");
    }
    assert!(RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(format!("{prefix}{ODM_KEY}"))
        .is_err());
    // A second unregister is not an error.
    unregister(&o).unwrap();
    let _ = std::fs::remove_dir_all(&tmp);
}
