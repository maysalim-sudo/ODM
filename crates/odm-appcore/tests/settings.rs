use odm_appcore::{AppPaths, Settings};
use std::time::Duration;

#[test]
fn defaults_match_the_spec() {
    let s = Settings::default();
    assert_eq!(
        (s.max_concurrent, s.max_connections, s.max_attempts),
        (3, 8, 5)
    );
    assert_eq!(
        (s.connect_timeout_s, s.read_timeout_s, s.speed_limit_kbps),
        (30, 60, 0)
    );
    assert_eq!(s.min_capture_bytes, 1 << 20);
    assert!(s.capture_enabled && s.close_to_tray && s.notifications && !s.start_minimized);
    assert_eq!(s.category_folders["video"], "Videos");
    assert_eq!(s.category_folders["general"], "");
    assert!(s.download_dir.ends_with("Downloads"));
}

#[test]
fn missing_file_gives_defaults_without_warnings() {
    let d = tempfile::tempdir().unwrap();
    let (s, warnings) = Settings::load(&d.path().join("settings.json"));
    assert_eq!(s, Settings::default());
    assert!(warnings.is_empty());
}

#[test]
fn corrupt_file_falls_back_to_defaults() {
    for text in [
        "{not json",
        "",
        "[1,2,3]",
        r#"{"max_concurrent": 5, "close_to"#,
    ] {
        let (s, warnings) = Settings::from_json(text);
        assert_eq!(s, Settings::default(), "input: {text:?}");
        assert_eq!(warnings.len(), 1, "input: {text:?}");
    }
}

#[test]
fn one_bad_field_keeps_the_rest() {
    let (s, warnings) = Settings::from_json(
        r#"{"max_concurrent": "many", "max_connections": 16, "close_to_tray": false}"#,
    );
    assert_eq!(s.max_concurrent, 3); // default kept
    assert_eq!(s.max_connections, 16);
    assert!(!s.close_to_tray);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("max_concurrent"), "{warnings:?}");
}

#[test]
fn unknown_fields_are_reported_and_ignored() {
    let (s, warnings) = Settings::from_json(r#"{"theme": "light", "max_attempts": 7}"#);
    assert_eq!(s.max_attempts, 7);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("theme"));
}

#[test]
fn out_of_range_values_are_clamped() {
    let (s, warnings) = Settings::from_json(
        r#"{"max_concurrent": 99, "max_connections": 0, "max_attempts": 0,
            "connect_timeout_s": 1, "read_timeout_s": 100000, "download_dir": "  "}"#,
    );
    assert_eq!(
        (s.max_concurrent, s.max_connections, s.max_attempts),
        (10, 1, 1)
    );
    assert_eq!((s.connect_timeout_s, s.read_timeout_s), (5, 300));
    assert_eq!(s.download_dir, Settings::default().download_dir);
    assert_eq!(warnings.len(), 6, "{warnings:?}");
}

#[test]
fn unsafe_category_folders_are_reset() {
    let (s, warnings) = Settings::from_json(
        r#"{"category_folders": {"video": "..\\..\\Windows", "audio": "C:\\x", "image": "Pics", "document": "a/b"}}"#,
    );
    assert_eq!(s.category_folders["video"], "Videos");
    assert_eq!(s.category_folders["audio"], "Music");
    assert_eq!(s.category_folders["document"], "Documents");
    assert_eq!(s.category_folders["image"], "Pics");
    // Categories missing from the file keep their defaults.
    assert_eq!(s.category_folders["compressed"], "Compressed");
    assert_eq!(warnings.len(), 3, "{warnings:?}");
}

#[test]
fn save_is_atomic_and_round_trips() {
    let d = tempfile::tempdir().unwrap();
    let p = AppPaths::at(d.path());
    p.ensure().unwrap();
    let s = Settings {
        max_connections: 12,
        skip_hosts: vec!["example.com".into()],
        ..Settings::default()
    };
    s.save(&p.settings).unwrap();
    let (back, warnings) = Settings::load(&p.settings);
    assert_eq!(back, s);
    assert!(warnings.is_empty());
    let leftovers: Vec<_> = std::fs::read_dir(d.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn to_engine_config_maps_units() {
    let s = Settings {
        speed_limit_kbps: 500,
        max_concurrent: 4,
        connect_timeout_s: 10,
        download_dir: "D:/dl".into(),
        ..Settings::default()
    };
    let c = s.to_engine_config();
    assert_eq!(c.speed_limit_bps, 500 * 1024);
    assert_eq!(c.max_concurrent, 4);
    assert_eq!(c.connect_timeout, Duration::from_secs(10));
    assert_eq!(c.download_dir, std::path::PathBuf::from("D:/dl"));
    assert_eq!(c.category_folders["video"], "Videos");
}

#[test]
fn paths_layout() {
    let p = AppPaths::at("X:/data");
    assert!(
        p.db.ends_with("odm.db")
            && p.settings.ends_with("settings.json")
            && p.logs.ends_with("logs")
    );
    assert!(AppPaths::default_location().data_dir.ends_with("ODM"));
}

#[test]
fn odd_shapes_fall_back_per_field() {
    let d = Settings::default();
    let cases = [
        (
            r#"{"max_concurrent": null, "max_attempts": 9}"#,
            "max_concurrent",
        ),
        (
            r#"{"max_concurrent": -1, "max_attempts": 9}"#,
            "max_concurrent",
        ),
        (
            r#"{"max_concurrent": "5", "max_attempts": 9}"#,
            "max_concurrent",
        ),
        (
            r#"{"category_folders": {"video": 5}, "max_attempts": 9}"#,
            "category_folders",
        ),
        (
            r#"{"skip_hosts": ["ok.com", 7], "max_attempts": 9}"#,
            "skip_hosts",
        ),
    ];
    for (text, bad) in cases {
        let (s, warnings) = Settings::from_json(text);
        assert_eq!(s.max_attempts, 9, "input: {text}");
        assert_eq!(s.max_concurrent, d.max_concurrent, "input: {text}");
        assert_eq!(s.category_folders, d.category_folders, "input: {text}");
        assert_eq!(s.skip_hosts, d.skip_hosts, "input: {text}");
        assert_eq!(warnings.len(), 1, "input: {text}: {warnings:?}");
        assert!(warnings[0].contains(bad), "input: {text}: {warnings:?}");
    }
}

#[test]
fn from_env_prefers_odm_data_dir() {
    // Env vars are process-wide: this is the only test that touches ODM_DATA_DIR.
    std::env::set_var("ODM_DATA_DIR", "X:/elsewhere");
    assert_eq!(AppPaths::from_env(), AppPaths::at("X:/elsewhere"));
    std::env::set_var("ODM_DATA_DIR", "");
    assert_eq!(AppPaths::from_env(), AppPaths::default_location());
    std::env::remove_var("ODM_DATA_DIR");
    assert_eq!(AppPaths::from_env(), AppPaths::default_location());
}

#[test]
fn clamp_normalizes_skip_hosts() {
    let mut s = Settings {
        skip_hosts: vec![
            "Example.com".into(),
            "https://example.com/".into(),
            "bad host!".into(),
        ],
        ..Settings::default()
    };
    let w = s.clamp();
    assert_eq!(s.skip_hosts, ["example.com"]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn ffmpeg_path_reaches_the_engine_config() {
    let mut s = odm_appcore::Settings::default();
    assert!(s.to_engine_config().ffmpeg_path.is_none());
    s.ffmpeg_path = "C:/tools/ffmpeg.exe".into();
    assert_eq!(
        s.to_engine_config().ffmpeg_path,
        Some(std::path::PathBuf::from("C:/tools/ffmpeg.exe"))
    );
}

#[test]
fn concurrent_saves_do_not_clash() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("settings.json");
    let variants: Vec<Settings> = (1..=8u32)
        .map(|n| Settings {
            max_concurrent: n,
            ..Settings::default()
        })
        .collect();
    std::thread::scope(|scope| {
        let handles: Vec<_> = variants
            .iter()
            .map(|s| {
                let path = path.clone();
                scope.spawn(move || s.save(&path))
            })
            .collect();
        for h in handles {
            h.join().unwrap().expect("every save succeeds");
        }
    });
    let (loaded, warnings) = Settings::load(&path);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(variants.contains(&loaded));
    let names: Vec<String> = std::fs::read_dir(d.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec!["settings.json".to_string()],
        "left over: {names:?}"
    );
}

#[test]
fn confirm_browser_downloads_defaults_true_and_parses() {
    assert!(Settings::default().confirm_browser_downloads);
    let (s, w) = Settings::from_json(r#"{"confirm_browser_downloads": false}"#);
    assert!(!s.confirm_browser_downloads && w.is_empty());
    let (s, w) = Settings::from_json(r#"{"confirm_browser_downloads": "no"}"#);
    assert!(s.confirm_browser_downloads);
    assert_eq!(w.len(), 1);
    let (s, w) = Settings::from_json("{}");
    assert!(s.confirm_browser_downloads && w.is_empty());
}
