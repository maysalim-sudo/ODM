use std::path::PathBuf;

/// Where the app keeps its files. `%APPDATA%\ODM` in production.
#[derive(Debug, Clone, PartialEq)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub db: PathBuf,
    pub settings: PathBuf,
    pub logs: PathBuf,
    /// Where the video tools (FFmpeg, yt-dlp, Deno) are kept: `<data_dir>/tools` when
    /// `ODM_DATA_DIR` is set, else `%LOCALAPPDATA%/ODM/tools`.
    pub tools_dir: PathBuf,
    /// Torrent session state, fastresume data and `.torrent` copies.
    pub torrents: PathBuf,
}

impl AppPaths {
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        let data_dir = dir.into();
        Self {
            db: data_dir.join("odm.db"),
            settings: data_dir.join("settings.json"),
            logs: data_dir.join("logs"),
            tools_dir: data_dir.join("tools"),
            torrents: data_dir.join("torrents"),
            data_dir,
        }
    }

    /// `ODM_DATA_DIR` if set and not empty, otherwise the default location.
    pub fn from_env() -> Self {
        match std::env::var_os("ODM_DATA_DIR") {
            Some(v) if !v.is_empty() => Self::at(PathBuf::from(v)),
            _ => Self::default_location(),
        }
    }

    pub fn default_location() -> Self {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let mut paths = Self::at(base.join(odm_core::ids::APP_DIR_NAME));
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            paths.tools_dir = PathBuf::from(local)
                .join(odm_core::ids::APP_DIR_NAME)
                .join("tools");
        }
        paths
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.logs)
    }
}
