//! Everything the desktop app needs that does not depend on Tauri.
pub mod api;
pub mod dto;
pub mod logging;
pub mod paths;
pub mod settings;

pub use paths::AppPaths;
pub use settings::Settings;
