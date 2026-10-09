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
    pub notifications: bool,
    /// Ask where to save (and whether to start) when the browser hands ODM a download.
    pub confirm_browser_downloads: bool,
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
            notifications: true,
            confirm_browser_downloads: true,
        }
    }
}

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
        let obj = match serde_json::from_str::<Value>(text) {
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
        let mut base = serde_json::to_value(Settings::default()).expect("settings serialize");
        for (key, value) in obj {
            if base.get(&key).is_none() {
                warnings.push(format!("{key}: unknown setting ignored"));
                continue;
            }
            let mut trial = base.clone();
            trial[&key] = value;
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
