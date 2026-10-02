use std::path::PathBuf;

/// Where the app keeps its files. `%APPDATA%\ODM` in production.
#[derive(Debug, Clone, PartialEq)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub db: PathBuf,
    pub settings: PathBuf,
    pub logs: PathBuf,
}

impl AppPaths {
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        let data_dir = dir.into();
        Self {
            db: data_dir.join("odm.db"),
            settings: data_dir.join("settings.json"),
            logs: data_dir.join("logs"),
            data_dir,
        }
    }

    pub fn default_location() -> Self {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self::at(base.join(odm_core::ids::APP_DIR_NAME))
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.logs)
    }
}
