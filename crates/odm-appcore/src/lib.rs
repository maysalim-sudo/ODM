//! Everything the desktop app needs that does not depend on Tauri.
pub mod api;
pub mod bridge;
pub mod capture;
pub mod dto;
pub mod ipc;
pub mod logging;
pub mod messages;
pub mod offers;
pub mod paths;
#[cfg(windows)]
pub mod pipe;
pub mod registration;
pub mod settings;
pub mod summary;

pub use paths::AppPaths;
pub use settings::Settings;
