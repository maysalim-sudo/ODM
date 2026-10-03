use odm_core::Category;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub download_dir: PathBuf,
    pub category_folders: HashMap<String, String>,
    pub max_concurrent: usize,
    pub max_connections: u32,
    pub max_attempts: u32,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub speed_limit_bps: u64,
    pub progress_interval: Duration,
    pub persist_interval: Duration,
    pub user_agent: String,
    /// FFmpeg executable used to merge HLS streams; PATH is searched when unset.
    pub ffmpeg_path: Option<PathBuf>,
    /// Whether to look for FFmpeg on `PATH` when `ffmpeg_path` is unset or missing.
    pub ffmpeg_search_path: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            download_dir: home.join("Downloads"),
            category_folders: Category::ALL
                .iter()
                .map(|c| (c.as_str().to_string(), c.default_folder().to_string()))
                .collect(),
            max_concurrent: 3,
            max_connections: 8,
            max_attempts: 5,
            connect_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(60),
            speed_limit_bps: 0,
            progress_interval: Duration::from_millis(250),
            persist_interval: Duration::from_secs(1),
            user_agent: format!("ODM/{}", env!("CARGO_PKG_VERSION")),
            ffmpeg_path: None,
            ffmpeg_search_path: true,
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
        self
    }

    pub fn save_dir_for(&self, category: &str) -> PathBuf {
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
