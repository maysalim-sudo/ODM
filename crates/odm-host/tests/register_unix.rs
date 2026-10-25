//! Linux: manifest files and the registration file round trip, in a temporary home.
#![cfg(unix)]
use odm_core::ids::{host_manifest_file, read_registration, registration_file, LINUX_HOST_DIRS};
use odm_host::register::{register, unregister, RegisterOptions};
use std::path::Path;

fn manifest(home: &Path, browser: &str) -> std::path::PathBuf {
    let (_, dir, _) = LINUX_HOST_DIRS
        .iter()
        .find(|(name, _, _)| *name == browser)
        .unwrap();
    home.join(dir).join(host_manifest_file())
}

fn read_json(p: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}

#[test]
fn register_then_unregister_round_trip() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    // Chrome and Firefox are "installed"; the other browsers are not.
    std::fs::create_dir_all(home.join(".config/google-chrome")).unwrap();
    std::fs::create_dir_all(home.join(".mozilla")).unwrap();
    let o = RegisterOptions {
        host_exe: "/opt/odm/odm-host".into(),
        app_exe: "/opt/odm/odm-app".into(),
        chromium_ids: vec![
            "abcdefghijklmnopabcdefghijklmnop".into(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        ],
        home: home.to_path_buf(),
    };

    register(&o).unwrap();
    let chrome = read_json(&manifest(home, "Chrome"));
    assert_eq!(chrome["path"], "/opt/odm/odm-host");
    assert_eq!(chrome["allowed_origins"].as_array().unwrap().len(), 2);
    let firefox = read_json(&manifest(home, "Firefox"));
    assert_eq!(firefox["path"], "/opt/odm/odm-host");
    assert_eq!(firefox["allowed_extensions"][0], "odm@odm.local");
    for absent in ["Edge", "Brave", "Vivaldi", "Opera", "Chromium", "Floorp"] {
        assert!(!manifest(home, absent).exists(), "{absent}");
        assert!(
            !manifest(home, absent).parent().unwrap().exists(),
            "{absent}"
        );
    }
    assert_eq!(
        read_registration(home, "HostPath").as_deref(),
        Some("/opt/odm/odm-host")
    );
    assert_eq!(
        read_registration(home, "AppPath").as_deref(),
        Some("/opt/odm/odm-app")
    );

    unregister(&o).unwrap();
    assert!(!manifest(home, "Chrome").exists());
    assert!(!manifest(home, "Firefox").exists());
    assert!(
        manifest(home, "Chrome").parent().unwrap().is_dir(),
        "folders stay"
    );
    assert!(!registration_file(home).exists());
    // A second unregister is not an error.
    unregister(&o).unwrap();
}
