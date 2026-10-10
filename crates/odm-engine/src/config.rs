use odm_core::Category;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::schedule::{active, LocalNow, Window};

/// What to do when a finished download's file name is already taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExistsPolicy {
    /// Keep both: the new file gets a number, `name (1).ext`.
    #[default]
    Rename,
    /// Replace the existing file.
    Overwrite,
    /// Pause the download and ask (`EngineEvent::ExistsPrompt`).
    Ask,
}

/// The user's answer to `EngineEvent::ExistsPrompt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExistsChoice {
    Replace,
    KeepBoth,
    Cancel,
}

/// How downloads reach the internet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ProxyConfig {
    /// The Windows proxy settings (and the `HTTP_PROXY`-style variables).
    #[default]
    System,
    /// No proxy at all.
    Direct,
    /// This proxy URL, already checked: `http`, `https`, `socks5` or `socks5h`.
    Manual(String),
}

/// The yt-dlp quality choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    #[default]
    Best,
    P1080,
    P720,
    P480,
    /// Audio only, saved as MP3.
    Audio,
}

/// The container yt-dlp merges video and audio into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Container {
    #[default]
    Mp4,
    Mkv,
}

/// What yt-dlp is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VideoOptions {
    pub quality: Quality,
    pub container: Container,
    /// Download the site's subtitles and embed them.
    pub subtitles: bool,
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub download_dir: PathBuf,
    pub category_folders: HashMap<String, String>,
    /// Save each type into its folder from `category_folders`; off saves everything in `download_dir`.
    pub sort_by_type: bool,
    pub max_concurrent: usize,
    pub max_connections: u32,
    pub max_attempts: u32,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub speed_limit_bps: u64,
    pub progress_interval: Duration,
    pub persist_interval: Duration,
    /// ODM's own User-Agent, sent when no custom one is set.
    pub user_agent: String,
    /// The user's User-Agent; it replaces `user_agent` and also goes to yt-dlp. A header
    /// captured from the browser still wins for that download.
    pub custom_user_agent: Option<String>,
    pub proxy: ProxyConfig,
    pub video: VideoOptions,
    /// FFmpeg executable used to merge HLS streams; PATH is searched when unset.
    pub ffmpeg_path: Option<PathBuf>,
    /// Whether to look for FFmpeg on `PATH` when `ffmpeg_path` is unset or missing.
    pub ffmpeg_search_path: bool,
    /// yt-dlp executable used for site videos; well-known places are searched when unset.
    pub ytdlp_path: Option<PathBuf>,
    /// Whether to look for yt-dlp on `PATH` and in the WinGet folders when `ytdlp_path` is unset or missing.
    pub ytdlp_search: bool,
    /// ODM's own folder for downloaded tools (`ffmpeg.exe`, `yt-dlp.exe`, `deno.exe`). It is
    /// searched after the configured paths and before `PATH`.
    pub tools_dir: Option<PathBuf>,
    pub on_exists: ExistsPolicy,
    /// When `speed_limit_bps` applies; None = always.
    pub limit_window: Option<Window>,
    /// When queued downloads may run; None = always. Running ones pause when it ends.
    pub queue_window: Option<Window>,
    /// Remove completed records older than this many days, once a day; 0 = never.
    pub history_days: u32,
    /// How often the manager checks the schedules and the history.
    pub timer_interval: Duration,
}

impl Default for EngineConfig {
    fn default() -> Self {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            download_dir: home.join("Downloads").join("ODM"),
            category_folders: Category::ALL
                .iter()
                .map(|c| (c.as_str().to_string(), c.default_folder().to_string()))
                .collect(),
            sort_by_type: true,
            max_concurrent: 3,
            max_connections: 8,
            max_attempts: 5,
            connect_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(60),
            speed_limit_bps: 0,
            progress_interval: Duration::from_millis(250),
            persist_interval: Duration::from_secs(1),
            user_agent: format!("ODM/{}", env!("CARGO_PKG_VERSION")),
            custom_user_agent: None,
            proxy: ProxyConfig::System,
            video: VideoOptions::default(),
            ffmpeg_path: None,
            ffmpeg_search_path: true,
            ytdlp_path: None,
            ytdlp_search: true,
            tools_dir: None,
            on_exists: ExistsPolicy::Rename,
            limit_window: None,
            queue_window: None,
            history_days: 0,
            timer_interval: Duration::from_secs(30),
        }
    }
}

impl EngineConfig {
    /// Clamp every field to its supported range (spec section 5 and 9).
    pub fn sanitized(mut self) -> Self {
        self.max_concurrent = self.max_concurrent.clamp(1, 10);
        self.max_connections = self.max_connections.clamp(1, 32);
        self.max_attempts = self.max_attempts.clamp(1, 20);
        self.connect_timeout = self
            .connect_timeout
            .clamp(Duration::from_secs(5), Duration::from_secs(120));
        self.read_timeout = self
            .read_timeout
            .clamp(Duration::from_secs(10), Duration::from_secs(300));
        self.progress_interval = self.progress_interval.max(Duration::from_millis(50));
        self.persist_interval = self.persist_interval.max(Duration::from_millis(100));
        self.timer_interval = self.timer_interval.max(Duration::from_millis(10));
        self
    }

    /// The User-Agent for ODM's own requests: the custom one, else ODM's.
    pub fn http_user_agent(&self) -> &str {
        self.custom_user_agent
            .as_deref()
            .unwrap_or(&self.user_agent)
    }

    /// The total speed limit in force at `now`: `speed_limit_bps` inside the limit
    /// window (or always, without one), unlimited (0) outside it.
    pub fn limit_at(&self, now: LocalNow) -> u64 {
        match &self.limit_window {
            Some(w) if !active(now, w) => 0,
            _ => self.speed_limit_bps,
        }
    }

    /// Whether queued downloads may start at `now`.
    pub fn queue_open_at(&self, now: LocalNow) -> bool {
        self.queue_window.as_ref().is_none_or(|w| active(now, w))
    }

    pub fn save_dir_for(&self, category: &str) -> PathBuf {
        if !self.sort_by_type {
            return self.download_dir.clone();
        }
        match self.category_folders.get(category).map(String::as_str) {
            Some(f) if !f.is_empty() => self.download_dir.join(f),
            _ => self.download_dir.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn defaults_and_category_dirs() {
        let c = super::EngineConfig::default();
        assert_eq!(
            (c.max_concurrent, c.max_connections, c.max_attempts),
            (3, 8, 5)
        );
        assert!(c.save_dir_for("video").ends_with("Videos"));
        assert_eq!(c.save_dir_for("general"), c.download_dir);
        assert_eq!(c.save_dir_for("unknown"), c.download_dir);
    }

    #[test]
    fn sort_by_type_off_saves_into_the_download_folder() {
        let c = super::EngineConfig {
            sort_by_type: false,
            ..Default::default()
        };
        assert_eq!(c.save_dir_for("video"), c.download_dir);
        assert_eq!(c.save_dir_for("general"), c.download_dir);
        assert!(super::EngineConfig::default().sort_by_type);
    }

    #[test]
    fn the_limit_applies_only_inside_its_window() {
        use crate::schedule::{LocalNow, Window};
        let at = |minute| LocalNow {
            unix_ms: 0,
            weekday: 0,
            minute,
        };
        let mut c = super::EngineConfig {
            speed_limit_bps: 1000,
            ..Default::default()
        };
        assert_eq!(c.limit_at(at(0)), 1000, "no window: always");
        c.limit_window = Some(Window {
            days: [true; 7],
            from: 9 * 60,
            to: 18 * 60,
        });
        assert_eq!(c.limit_at(at(9 * 60)), 1000);
        assert_eq!(c.limit_at(at(18 * 60)), 0);
        assert_eq!(c.limit_at(at(3 * 60)), 0);
        assert!(c.queue_open_at(at(3 * 60)), "no queue window: always open");
        c.queue_window = c.limit_window;
        assert!(!c.queue_open_at(at(3 * 60)));
        assert!(c.queue_open_at(at(10 * 60)));
    }

    #[test]
    fn sanitized_clamps_out_of_range_values() {
        use std::time::Duration;
        let c = super::EngineConfig {
            max_concurrent: 0,
            max_connections: 500,
            max_attempts: 0,
            connect_timeout: Duration::ZERO,
            read_timeout: Duration::from_secs(9999),
            progress_interval: Duration::ZERO,
            persist_interval: Duration::ZERO,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(
            (c.max_concurrent, c.max_connections, c.max_attempts),
            (1, 32, 1)
        );
        assert_eq!(c.connect_timeout, Duration::from_secs(5));
        assert_eq!(c.read_timeout, Duration::from_secs(300));
        assert_eq!(c.progress_interval, Duration::from_millis(50));
        assert_eq!(c.persist_interval, Duration::from_millis(100));
        let d = super::EngineConfig::default();
        assert_eq!(d.clone().sanitized().max_concurrent, d.max_concurrent);
    }
}
