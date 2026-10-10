use crate::capture::normalize_host;
use odm_core::Category;
use odm_engine::EngineConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub download_dir: String,
    pub category_folders: BTreeMap<String, String>,
    pub max_concurrent: u32,
    pub max_connections: u32,
    pub max_attempts: u32,
    /// KB/s; 0 = unlimited.
    pub speed_limit_kbps: u64,
    pub connect_timeout_s: u64,
    pub read_timeout_s: u64,
    pub ffmpeg_path: String,
    pub ytdlp_path: String,
    pub capture_enabled: bool,
    pub min_capture_bytes: u64,
    pub skip_hosts: Vec<String>,
    pub close_to_tray: bool,
    pub start_minimized: bool,
    /// Ask where to save (and whether to start) when the browser hands ODM a download.
    pub confirm_browser_downloads: bool,
    /// The first-run welcome screen has been completed.
    pub onboarding_done: bool,
    // Added with the Settings page. Older files lack these and get the defaults.
    pub start_with_windows: bool,
    /// Notify when a download finishes. Replaces the old `notifications` switch.
    pub notify_complete: bool,
    /// Notify when a download fails.
    pub notify_failed: bool,
    /// Keep the PC awake while anything downloads.
    pub prevent_sleep: bool,
    /// Save each type into its folder from `category_folders`; off saves everything in `download_dir`.
    pub sort_by_type: bool,
    pub on_exists: OnExists,
    pub on_complete: OnComplete,
    pub start_mode: StartMode,
    /// Remove finished downloads from the list after this many days; 0 = never.
    pub history_days: u32,
    /// When the speed limit applies, if enabled; otherwise it always applies.
    pub limit_schedule: Schedule,
    /// When the queue runs, if enabled; otherwise it always runs.
    pub queue_schedule: Schedule,
    pub after_queue: AfterQueue,
    /// File extensions left to the browser, lower case without the dot.
    pub skip_types: Vec<String>,
    pub video_quality: VideoQuality,
    pub video_container: VideoContainer,
    pub video_subtitles: bool,
    pub ytdlp_auto_update: bool,
    pub proxy_mode: ProxyMode,
    /// Used when `proxy_mode` is manual: `http://host:port` or `socks5://host:port`.
    pub proxy_url: String,
    /// Empty = ODM's default User-Agent.
    pub user_agent: String,
    pub update_check: bool,
    pub update_install: UpdateInstall,
}

/// What to do when the target file already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnExists {
    #[default]
    Rename,
    Overwrite,
    Ask,
}

/// What to do after a download finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnComplete {
    #[default]
    Nothing,
    OpenFolder,
    OpenFile,
}

/// Whether new downloads start right away or are added paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartMode {
    #[default]
    Now,
    Paused,
}

/// What to do once the scheduled queue is empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AfterQueue {
    #[default]
    Nothing,
    Sleep,
    Shutdown,
    Quit,
}

/// The preferred yt-dlp quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VideoQuality {
    #[default]
    #[serde(rename = "best")]
    Best,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "480p")]
    P480,
    #[serde(rename = "audio")]
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoContainer {
    #[default]
    Mp4,
    Mkv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    #[default]
    System,
    None,
    Manual,
}

/// What to do once an update is ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateInstall {
    #[default]
    Ask,
    Auto,
}

/// A weekly time window. `days` runs Monday (index 0) to Sunday (6). `from` and `to`
/// are local "HH:MM"; a `to` earlier than `from` means the window crosses midnight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Schedule {
    pub enabled: bool,
    pub days: [bool; 7],
    pub from: String,
    pub to: String,
}

impl Schedule {
    fn new(from: &str, to: &str) -> Self {
        Self {
            enabled: false,
            days: [true; 7],
            from: from.into(),
            to: to.into(),
        }
    }

    /// Resets each time that is not "HH:MM" to the one in `def`, with a warning.
    fn fix_times(&mut self, name: &str, def: &Schedule, w: &mut Vec<String>) {
        for (field, value, fallback) in [
            ("from", &mut self.from, &def.from),
            ("to", &mut self.to, &def.to),
        ] {
            if parse_hhmm(value).is_none() {
                w.push(format!(
                    "{name}.{field}: {value:?} is not a HH:MM time; using {fallback}"
                ));
                *value = fallback.clone();
            }
        }
    }
}

impl Default for Schedule {
    fn default() -> Self {
        Self::new("00:00", "06:00")
    }
}

fn default_limit_schedule() -> Schedule {
    Schedule::new("09:00", "18:00")
}

fn default_queue_schedule() -> Schedule {
    Schedule::new("01:00", "07:00")
}

/// Minutes after midnight for an exact "HH:MM" (two digits each, 00:00 to 23:59).
pub fn parse_hhmm(text: &str) -> Option<u16> {
    let (h, m) = text.split_once(':')?;
    let digits = |s: &str| s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(h) || !digits(m) {
        return None;
    }
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// A file type as the skip list keeps it: lower case, no dot. None if it is not a plain extension.
fn normalize_type(t: &str) -> Option<String> {
    let t = t.trim().trim_start_matches('.').to_ascii_lowercase();
    (!t.is_empty() && t.len() <= 16 && t.bytes().all(|b| b.is_ascii_alphanumeric())).then_some(t)
}

fn default_folders() -> BTreeMap<String, String> {
    Category::ALL
        .iter()
        .map(|c| (c.as_str().to_string(), c.default_folder().to_string()))
        .collect()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: EngineConfig::default()
                .download_dir
                .to_string_lossy()
                .into_owned(),
            category_folders: default_folders(),
            max_concurrent: 3,
            max_connections: 8,
            max_attempts: 5,
            speed_limit_kbps: 0,
            connect_timeout_s: 30,
            read_timeout_s: 60,
            ffmpeg_path: String::new(),
            ytdlp_path: String::new(),
            capture_enabled: true,
            min_capture_bytes: 1 << 20,
            skip_hosts: Vec::new(),
            close_to_tray: true,
            start_minimized: false,
            confirm_browser_downloads: true,
            onboarding_done: false,
            start_with_windows: false,
            notify_complete: true,
            notify_failed: true,
            prevent_sleep: true,
            sort_by_type: true,
            on_exists: OnExists::default(),
            on_complete: OnComplete::default(),
            start_mode: StartMode::default(),
            history_days: 0,
            limit_schedule: default_limit_schedule(),
            queue_schedule: default_queue_schedule(),
            after_queue: AfterQueue::default(),
            skip_types: Vec::new(),
            video_quality: VideoQuality::default(),
            video_container: VideoContainer::default(),
            video_subtitles: false,
            ytdlp_auto_update: true,
            proxy_mode: ProxyMode::default(),
            proxy_url: String::new(),
            user_agent: String::new(),
            update_check: true,
            update_install: UpdateInstall::default(),
        }
    }
}

/// The single notifications switch of older settings files.
const LEGACY_NOTIFICATIONS: &str = "notifications";

fn clamp_num<T: PartialOrd + Copy + std::fmt::Display>(
    name: &str,
    v: &mut T,
    lo: T,
    hi: T,
    w: &mut Vec<String>,
) {
    if *v < lo || *v > hi {
        let fixed = if *v < lo { lo } else { hi };
        w.push(format!("{name}: {v} is outside {lo}..{hi}; using {fixed}"));
        *v = fixed;
    }
}

/// A category folder must be a single plain folder name under the download directory.
fn safe_folder(name: &str) -> bool {
    !name.contains(['\\', '/', ':']) && name != ".." && name != "." && name.trim() == name
}

impl Settings {
    /// Parse leniently: each bad or unknown field is reported and the default kept.
    pub fn from_json(text: &str) -> (Settings, Vec<String>) {
        let mut warnings = Vec::new();
        let mut obj = match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(o)) => o,
            Ok(_) => {
                return (
                    Settings::default(),
                    vec!["settings file is not a JSON object; using defaults".into()],
                );
            }
            Err(e) => {
                return (
                    Settings::default(),
                    vec![format!(
                        "settings file is not valid JSON ({e}); using defaults"
                    )],
                )
            }
        };
        // Files from before the Settings page had one switch for all notifications.
        if let Some(old) = obj.remove(LEGACY_NOTIFICATIONS) {
            if old.is_boolean() {
                for key in ["notify_complete", "notify_failed"] {
                    obj.entry(key).or_insert_with(|| old.clone());
                }
            } else {
                warnings.push(format!(
                    "{LEGACY_NOTIFICATIONS}: invalid value; using the default"
                ));
            }
        }
        let mut base = serde_json::to_value(Settings::default()).expect("settings serialize");
        for (key, value) in obj {
            if base.get(&key).is_none() {
                warnings.push(format!("{key}: unknown setting ignored"));
                continue;
            }
            let mut trial = base.clone();
            match (&mut trial[&key], value) {
                // A partial object (a schedule with only `enabled`) keeps that field's
                // defaults for the keys it lacks.
                (Value::Object(def), Value::Object(given)) => def.extend(given),
                (slot, value) => *slot = value,
            }
            if serde_json::from_value::<Settings>(trial.clone()).is_ok() {
                base = trial;
            } else {
                warnings.push(format!("{key}: invalid value; using the default"));
            }
        }
        let mut s: Settings = serde_json::from_value(base).expect("validated above");
        warnings.extend(s.clamp());
        (s, warnings)
    }

    pub fn clamp(&mut self) -> Vec<String> {
        let mut w = Vec::new();
        clamp_num("max_concurrent", &mut self.max_concurrent, 1, 10, &mut w);
        clamp_num("max_connections", &mut self.max_connections, 1, 32, &mut w);
        clamp_num("max_attempts", &mut self.max_attempts, 1, 20, &mut w);
        clamp_num(
            "connect_timeout_s",
            &mut self.connect_timeout_s,
            5,
            120,
            &mut w,
        );
        clamp_num("read_timeout_s", &mut self.read_timeout_s, 10, 300, &mut w);
        if self.download_dir.trim().is_empty() {
            w.push("download_dir: empty; using the default".into());
            self.download_dir = Settings::default().download_dir;
        }
        let defaults = default_folders();
        for (cat, def) in &defaults {
            match self.category_folders.get(cat) {
                Some(f) if safe_folder(f) => {}
                Some(f) => {
                    w.push(format!(
                        "category_folders.{cat}: {f:?} is not a plain folder name; using {def:?}"
                    ));
                    self.category_folders.insert(cat.clone(), def.clone());
                }
                None => {
                    self.category_folders.insert(cat.clone(), def.clone());
                }
            }
        }
        self.category_folders
            .retain(|k, _| defaults.contains_key(k));
        let mut hosts: Vec<String> = Vec::new();
        for h in self.skip_hosts.iter().filter_map(|e| normalize_host(e)) {
            if !hosts.contains(&h) {
                hosts.push(h);
            }
        }
        self.skip_hosts = hosts;
        clamp_num("history_days", &mut self.history_days, 0, 3650, &mut w);
        self.limit_schedule
            .fix_times("limit_schedule", &default_limit_schedule(), &mut w);
        self.queue_schedule
            .fix_times("queue_schedule", &default_queue_schedule(), &mut w);
        let mut types: Vec<String> = Vec::new();
        for t in &self.skip_types {
            match normalize_type(t) {
                Some(t) => {
                    if !types.contains(&t) {
                        types.push(t);
                    }
                }
                None if t.trim().is_empty() => {}
                None => w.push(format!("skip_types: {t:?} is not a file type; ignored")),
            }
        }
        self.skip_types = types;
        w
    }

    pub fn load(path: &Path) -> (Settings, Vec<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => Settings::from_json(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Settings::default(), Vec::new()),
            Err(e) => (
                Settings::default(),
                vec![format!("cannot read settings ({e}); using defaults")],
            ),
        }
    }

    /// Write to a temp file, then rename over the real one.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        let tmp = path.with_file_name(format!(
            "{name}.{}-{}.tmp",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&serde_json::to_vec_pretty(self).expect("settings serialize"))?;
            // Flush to disk before the rename so a crash can't leave a truncated file.
            f.sync_all()?;
        }
        let renamed = std::fs::rename(&tmp, path);
        if renamed.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        renamed
    }

    /// The engine configuration, with the tools folder of the running app (`AppPaths::from_env`).
    pub fn to_engine_config(&self) -> EngineConfig {
        self.to_engine_config_with(crate::AppPaths::from_env().tools_dir)
    }

    pub fn to_engine_config_with(&self, tools_dir: PathBuf) -> EngineConfig {
        EngineConfig {
            tools_dir: Some(tools_dir),
            download_dir: PathBuf::from(&self.download_dir),
            category_folders: self.category_folders.clone().into_iter().collect(),
            sort_by_type: self.sort_by_type,
            max_concurrent: self.max_concurrent as usize,
            max_connections: self.max_connections,
            max_attempts: self.max_attempts,
            connect_timeout: Duration::from_secs(self.connect_timeout_s),
            read_timeout: Duration::from_secs(self.read_timeout_s),
            speed_limit_bps: self.speed_limit_kbps.saturating_mul(1024),
            ffmpeg_path: Some(self.ffmpeg_path.trim())
                .filter(|p| !p.is_empty())
                .map(PathBuf::from),
            ytdlp_path: Some(self.ytdlp_path.trim())
                .filter(|p| !p.is_empty())
                .map(PathBuf::from),
            ..EngineConfig::default()
        }
        .sanitized()
    }
}
