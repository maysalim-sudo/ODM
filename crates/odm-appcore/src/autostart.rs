//! "Start ODM with Windows". The app implements this with `tauri-plugin-autostart` (an HKCU
//! Run entry that starts ODM with `--background`); tests use a fake.
pub trait Autostart: Send + Sync {
    /// Adds (`true`) or removes (`false`) the Windows sign-in entry.
    fn set_enabled(&self, on: bool) -> Result<(), String>;
}
