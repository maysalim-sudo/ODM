use odm_appcore::settings::{
    parse_hhmm, AfterQueue, OnComplete, OnExists, ProxyMode, Schedule, StartMode, UpdateInstall,
    VideoContainer, VideoQuality,
};
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
    assert!(s.capture_enabled && s.close_to_tray && s.notify_complete && !s.start_minimized);
    assert_eq!(s.category_folders["video"], "Videos");
    assert_eq!(s.category_folders["general"], "");
    assert!(std::path::Path::new(&s.download_dir).ends_with("Downloads/ODM"));
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
fn engine_config_points_at_the_tools_dir() {
    let c = Settings::default().to_engine_config_with(std::path::PathBuf::from("X:/data/tools"));
    assert_eq!(c.tools_dir, Some(std::path::PathBuf::from("X:/data/tools")));
    assert_eq!(
        AppPaths::at("X:/data").tools_dir,
        std::path::PathBuf::from("X:/data").join("tools")
    );
    // The plain form uses the app's own tools folder.
    assert!(Settings::default().to_engine_config().tools_dir.is_some());
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

#[test]
fn ytdlp_path_reaches_the_engine_config() {
    let mut s = odm_appcore::Settings::default();
    assert!(s.ytdlp_path.is_empty());
    assert!(s.to_engine_config().ytdlp_path.is_none());
    s.ytdlp_path = "  C:/tools/yt-dlp.exe ".into();
    assert_eq!(
        s.to_engine_config().ytdlp_path,
        Some(std::path::PathBuf::from("C:/tools/yt-dlp.exe"))
    );
}

#[test]
fn onboarding_done_defaults_false_and_round_trips() {
    assert!(!Settings::default().onboarding_done);
    let (s, w) = Settings::from_json(r#"{"onboarding_done": true}"#);
    assert!(w.is_empty(), "{w:?}");
    assert!(s.onboarding_done);
    // Settings files written before the welcome screen existed lack the field.
    let (old, w) = Settings::from_json(r#"{"max_concurrent": 2}"#);
    assert!(w.is_empty());
    assert!(!old.onboarding_done);
}

#[test]
fn default_download_dir_is_downloads_odm() {
    let engine = odm_engine::EngineConfig::default().download_dir;
    assert!(engine.ends_with("Downloads/ODM"), "{engine:?}");
    assert_eq!(
        std::path::PathBuf::from(Settings::default().download_dir),
        engine
    );
}

/// A settings file written before the Settings page: none of the new keys, the old
/// `notifications` switch, and the user's own folder.
const OLD_FILE: &str = r#"{
  "download_dir": "D:\\Stuff\\Downloads",
  "category_folders": {"audio": "Music", "compressed": "Compressed", "document": "Documents",
    "general": "", "image": "Images", "program": "Programs", "video": "Videos"},
  "max_concurrent": 4, "max_connections": 8, "max_attempts": 5, "speed_limit_kbps": 0,
  "connect_timeout_s": 30, "read_timeout_s": 60, "ffmpeg_path": "", "ytdlp_path": "",
  "capture_enabled": true, "min_capture_bytes": 1048576, "skip_hosts": [],
  "close_to_tray": true, "start_minimized": false, "notifications": true,
  "confirm_browser_downloads": true, "onboarding_done": true
}"#;

#[test]
fn old_settings_file_keeps_folder_and_gets_new_defaults() {
    let (s, warnings) = Settings::from_json(OLD_FILE);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(s.download_dir, "D:\\Stuff\\Downloads");
    assert_eq!(
        s,
        Settings {
            download_dir: "D:\\Stuff\\Downloads".into(),
            max_concurrent: 4,
            onboarding_done: true,
            ..Settings::default()
        }
    );
}

#[test]
fn new_fields_have_the_documented_defaults() {
    let s = Settings::default();
    assert!(!s.start_with_windows);
    assert!(s.notify_complete && s.notify_failed && s.prevent_sleep && s.sort_by_type);
    assert_eq!(s.on_exists, OnExists::Rename);
    assert_eq!(s.on_complete, OnComplete::Nothing);
    assert_eq!(s.start_mode, StartMode::Now);
    assert_eq!(s.history_days, 0);
    assert!(!s.limit_schedule.enabled && !s.queue_schedule.enabled);
    assert_eq!(s.limit_schedule.days, [true; 7]);
    assert_eq!(s.after_queue, AfterQueue::Nothing);
    assert!(s.skip_types.is_empty());
    assert_eq!(s.video_quality, VideoQuality::Best);
    assert_eq!(s.video_container, VideoContainer::Mp4);
    assert!(!s.video_subtitles && s.ytdlp_auto_update);
    assert_eq!(s.proxy_mode, ProxyMode::System);
    assert!(s.proxy_url.is_empty() && s.user_agent.is_empty());
    assert!(s.update_check);
    assert_eq!(s.update_install, UpdateInstall::Ask);
}

#[test]
fn choice_fields_use_snake_case_and_fall_back_when_unknown() {
    let (s, w) = Settings::from_json(
        r#"{"on_exists": "ask", "on_complete": "open_folder", "start_mode": "paused",
            "after_queue": "shutdown", "video_quality": "720p", "video_container": "mkv",
            "proxy_mode": "manual", "update_install": "auto"}"#,
    );
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(s.on_exists, OnExists::Ask);
    assert_eq!(s.on_complete, OnComplete::OpenFolder);
    assert_eq!(s.start_mode, StartMode::Paused);
    assert_eq!(s.after_queue, AfterQueue::Shutdown);
    assert_eq!(s.video_quality, VideoQuality::P720);
    assert_eq!(s.video_container, VideoContainer::Mkv);
    assert_eq!(s.proxy_mode, ProxyMode::Manual);
    assert_eq!(s.update_install, UpdateInstall::Auto);
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(json["on_complete"], "open_folder");
    assert_eq!(json["video_quality"], "720p");
    for q in ["best", "1080p", "480p", "audio"] {
        let (s, w) = Settings::from_json(&format!(r#"{{"video_quality": "{q}"}}"#));
        assert!(w.is_empty(), "{q}: {w:?}");
        assert_eq!(serde_json::to_value(s.video_quality).unwrap(), q);
    }
    let (s, w) = Settings::from_json(r#"{"on_exists": "explode", "max_attempts": 9}"#);
    assert_eq!(s.on_exists, OnExists::Rename);
    assert_eq!(s.max_attempts, 9);
    assert_eq!(w.len(), 1);
    assert!(w[0].contains("on_exists"), "{w:?}");
}

#[test]
fn legacy_notifications_switch_maps_to_both_new_ones() {
    let (s, w) = Settings::from_json(r#"{"notifications": false}"#);
    assert!(w.is_empty(), "{w:?}");
    assert!(!s.notify_complete && !s.notify_failed);
    // The new keys win when both are present.
    let (s, w) = Settings::from_json(r#"{"notifications": false, "notify_failed": true}"#);
    assert!(w.is_empty(), "{w:?}");
    assert!(!s.notify_complete && s.notify_failed);
    let (s, w) = Settings::from_json(r#"{"notifications": "yes"}"#);
    assert!(s.notify_complete && s.notify_failed);
    assert_eq!(w.len(), 1, "{w:?}");
    // It is not written back.
    let json = serde_json::to_value(Settings::default()).unwrap();
    assert!(json.get("notifications").is_none());
}

#[test]
fn schedules_parse_and_bad_times_fall_back() {
    let (s, w) = Settings::from_json(
        r#"{"limit_schedule": {"enabled": true, "days": [true,true,true,true,true,false,false],
            "from": "22:00", "to": "06:30"}}"#,
    );
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(
        s.limit_schedule,
        Schedule {
            enabled: true,
            days: [true, true, true, true, true, false, false],
            from: "22:00".into(),
            to: "06:30".into(),
        }
    );
    // A partial object keeps the defaults for the rest.
    let (s, w) = Settings::from_json(r#"{"queue_schedule": {"enabled": true}}"#);
    assert!(w.is_empty(), "{w:?}");
    assert!(s.queue_schedule.enabled);
    assert_eq!(
        s.queue_schedule.from,
        Settings::default().queue_schedule.from
    );
    // A wrong number of days is an invalid value.
    let (s, w) = Settings::from_json(r#"{"queue_schedule": {"days": [true]}}"#);
    assert_eq!(s.queue_schedule, Settings::default().queue_schedule);
    assert_eq!(w.len(), 1, "{w:?}");
    // A time that is not HH:MM resets that time only.
    let (s, w) = Settings::from_json(
        r#"{"limit_schedule": {"enabled": true, "from": "25:00", "to": "7:5"}}"#,
    );
    assert!(s.limit_schedule.enabled);
    assert_eq!(
        s.limit_schedule.from,
        Settings::default().limit_schedule.from
    );
    assert_eq!(s.limit_schedule.to, Settings::default().limit_schedule.to);
    assert_eq!(w.len(), 2, "{w:?}");
}

#[test]
fn parse_hhmm_reads_minutes_after_midnight() {
    assert_eq!(parse_hhmm("00:00"), Some(0));
    assert_eq!(parse_hhmm("06:30"), Some(390));
    assert_eq!(parse_hhmm("23:59"), Some(1439));
    for bad in ["24:00", "7:05", "07:60", "0700", "", "aa:bb", "07:05 "] {
        assert_eq!(parse_hhmm(bad), None, "{bad:?}");
    }
}

#[test]
fn skip_types_and_history_days_are_normalized() {
    let mut s = Settings {
        skip_types: vec![
            ".PDF".into(),
            " jpg ".into(),
            "pdf".into(),
            "bad type!".into(),
            "".into(),
        ],
        history_days: 100_000,
        ..Settings::default()
    };
    let w = s.clamp();
    assert_eq!(s.skip_types, ["pdf", "jpg"]);
    assert_eq!(s.history_days, 90);
    assert_eq!(w.len(), 2, "{w:?}"); // the bad type and the snap
}

#[test]
fn sort_by_type_reaches_the_engine_config() {
    assert!(Settings::default().to_engine_config().sort_by_type);
    let off = Settings {
        sort_by_type: false,
        download_dir: "D:/dl".into(),
        ..Settings::default()
    };
    let c = off.to_engine_config();
    assert!(!c.sort_by_type);
    assert_eq!(c.save_dir_for("video"), std::path::PathBuf::from("D:/dl"));
}

#[test]
fn history_days_snap_to_the_offered_choices() {
    // The page offers Never (0), 7, 30 and 90 days.
    for (given, want) in [
        (0, 0),
        (7, 7),
        (30, 30),
        (90, 90),
        (1, 7),
        (18, 7),
        (19, 30),
        (59, 30),
        // Halfway keeps the longer history.
        (60, 90),
        (365, 90),
        (u32::MAX, 90),
    ] {
        let mut s = Settings {
            history_days: given,
            ..Settings::default()
        };
        let w = s.clamp();
        assert_eq!(s.history_days, want, "{given} days");
        assert_eq!(w.is_empty(), given == want, "{given}: {w:?}");
    }
    let (s, w) = Settings::from_json(r#"{"history_days": 45}"#);
    assert_eq!((s.history_days, w.len()), (30, 1));
}

#[test]
fn behaviour_settings_reach_the_engine_config() {
    use odm_engine::schedule::Window;
    use odm_engine::ExistsPolicy;
    let d = Settings::default().to_engine_config();
    assert_eq!(d.on_exists, ExistsPolicy::Rename);
    assert_eq!((d.limit_window, d.queue_window), (None, None));
    assert_eq!(d.history_days, 0);
    assert_eq!(d.timer_interval, Duration::from_secs(30));

    let s = Settings {
        on_exists: OnExists::Ask,
        history_days: 30,
        limit_schedule: Schedule {
            enabled: true,
            days: [true, true, true, true, true, false, false],
            from: "09:30".into(),
            to: "17:00".into(),
        },
        queue_schedule: Schedule {
            enabled: true,
            days: [false, false, false, false, false, true, true],
            from: "22:00".into(),
            to: "06:00".into(),
        },
        ..Settings::default()
    };
    let c = s.to_engine_config();
    assert_eq!(c.on_exists, ExistsPolicy::Ask);
    assert_eq!(c.history_days, 30);
    assert_eq!(
        c.limit_window,
        Some(Window {
            days: [true, true, true, true, true, false, false],
            from: 9 * 60 + 30,
            to: 17 * 60,
        })
    );
    assert_eq!(
        c.queue_window,
        Some(Window {
            days: [false, false, false, false, false, true, true],
            from: 22 * 60,
            to: 6 * 60,
        })
    );
    let overwrite = Settings {
        on_exists: OnExists::Overwrite,
        ..Settings::default()
    };
    assert_eq!(
        overwrite.to_engine_config().on_exists,
        ExistsPolicy::Overwrite
    );
}

#[test]
fn a_schedule_without_days_is_turned_off() {
    let mut s = Settings {
        queue_schedule: Schedule {
            enabled: true,
            days: [false; 7],
            from: "01:00".into(),
            to: "07:00".into(),
        },
        ..Settings::default()
    };
    // Even unclamped, it never reaches the engine as a window that can never open.
    assert_eq!(s.to_engine_config().queue_window, None);
    let w = s.clamp();
    assert!(!s.queue_schedule.enabled);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("queue_schedule"), "{w:?}");
    let (s, w) = Settings::from_json(
        r#"{"limit_schedule": {"enabled": true, "days": [false,false,false,false,false,false,false]}}"#,
    );
    assert!(!s.limit_schedule.enabled);
    assert_eq!(w.len(), 1, "{w:?}");
}
