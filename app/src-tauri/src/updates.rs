//! Automatic updates: looks for a newer ODM on GitHub, downloads it, checks its signature and
//! installs it through the same shutdown as quitting, so downloads pause and resume afterwards.
//!
//! A build without a public key (any build not made by `release.mjs --publish-ready`) never
//! checks: the Settings page says updates are not available in it.

use odm_appcore::dto::UiEvent;
use odm_appcore::settings::{ProxyMode, UpdateInstall};
use odm_appcore::Settings;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime, Url};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::{Error, Update, Updater, UpdaterExt};

use crate::AppState;

/// Test hook: a different `latest.json`. The signature is still required.
pub const ENDPOINT_ENV: &str = "ODM_UPDATE_ENDPOINT";
pub const FIRST_CHECK: Duration = Duration::from_secs(10);
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// How long one check may take before it counts as failed.
const CHECK_TIMEOUT: Duration = Duration::from_secs(60);

pub const VERIFY_FAILED: &str = "The update couldn't be verified, so it wasn't installed.";
pub const CHECK_FAILED: &str =
    "ODM couldn't check for updates. Check your internet connection and try again.";
pub const DOWNLOAD_FAILED: &str = "The update couldn't be downloaded. Try again later.";
pub const INSTALL_FAILED: &str =
    "The update couldn't be installed. ODM will start again as it was.";
pub const UNAVAILABLE: &str = "Updates are not available in this build.";

/// The public key compiled into this build: `plugins.updater.pubkey`, empty when there is none.
pub fn pubkey_in(plugins: &HashMap<String, Value>) -> String {
    plugins
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub fn has_pubkey(pubkey: &str) -> bool {
    !pubkey.trim().is_empty()
}

/// Whether to check now: never without a key; a manual "Check now" ignores the setting.
pub fn should_check(has_key: bool, update_check: bool, manual: bool) -> bool {
    has_key && (manual || update_check)
}

/// The wait before automatic check number `n` (0 is the first after start).
pub fn check_delay(n: u32) -> Duration {
    if n == 0 {
        FIRST_CHECK
    } else {
        CHECK_EVERY
    }
}

/// True when `candidate` is a newer version than `current`. Unreadable versions never are.
pub fn is_newer(current: &str, candidate: &str) -> bool {
    let parse = |v: &str| semver::Version::parse(v.trim().trim_start_matches('v')).ok();
    match (parse(current), parse(candidate)) {
        (Some(c), Some(n)) => n > c,
        _ => false,
    }
}

/// An endpoint from `ODM_UPDATE_ENDPOINT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub url: Url,
    /// Plain http to this PC, which the updater otherwise refuses in release builds.
    pub loopback_http: bool,
}

/// The override, if it is an https address or an http one on this PC; anything else is ignored.
pub fn endpoint_override(var: Option<&str>) -> Option<Endpoint> {
    let url = Url::parse(var?.trim()).ok()?;
    let loopback = matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("[::1]")
    );
    match url.scheme() {
        "https" => Some(Endpoint {
            url,
            loopback_http: false,
        }),
        "http" if loopback => Some(Endpoint {
            url,
            loopback_http: true,
        }),
        _ => None,
    }
}

/// Lets the updater use a loopback http override; returns the compiled-in public key.
pub fn prepare_config(plugins: &mut HashMap<String, Value>, endpoint: Option<&Endpoint>) -> String {
    if endpoint.is_some_and(|e| e.loopback_http) {
        if let Some(Value::Object(updater)) = plugins.get_mut("updater") {
            updater.insert(
                "dangerousInsecureTransportProtocol".into(),
                Value::Bool(true),
            );
        }
    }
    pubkey_in(plugins)
}

/// What ODM remembers between runs, in `updates.json` next to the settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Memory {
    /// The last version a Windows notification announced.
    pub notified: Option<String>,
    /// The version whose banner "Later" hid.
    pub dismissed: Option<String>,
    pub last_checked_ms: Option<u64>,
}

impl Memory {
    /// The saved state; a missing or damaged file starts over.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Written to a temporary file first, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)
    }

    /// True the first time `version` is seen, so each version is announced once.
    pub fn take_notify(&mut self, version: &str) -> bool {
        if self.notified.as_deref() == Some(version) {
            return false;
        }
        self.notified = Some(version.to_string());
        true
    }

    pub fn dismiss(&mut self, version: &str) {
        self.dismissed = Some(version.to_string());
    }

    pub fn is_dismissed(&self, version: &str) -> bool {
        self.dismissed.as_deref() == Some(version)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Check,
    Download,
    Install,
}

/// The message for the user. Any signature problem is a verification failure.
pub fn user_error(stage: Stage, e: &Error) -> &'static str {
    match e {
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => VERIFY_FAILED,
        _ => match stage {
            Stage::Check => CHECK_FAILED,
            Stage::Download => DOWNLOAD_FAILED,
            Stage::Install => INSTALL_FAILED,
        },
    }
}

/// An update as the window shows it. `notes` is plain text, never HTML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    /// RFC 3339, empty when the manifest has none.
    pub date: String,
}

fn info_of(u: &Update) -> UpdateInfo {
    UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone().unwrap_or_default().trim().to_string(),
        date: u
            .raw_json
            .get("pub_date")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

/// What the Settings page and the banner need to know.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    /// False in a build without a public key: no checks, no banner.
    pub supported: bool,
    pub current: String,
    pub last_checked_ms: Option<u64>,
    pub available: Option<UpdateInfo>,
    /// "Later" hid the banner for this version.
    pub dismissed: bool,
    pub downloading: bool,
    /// Downloaded and verified, waiting to be installed.
    pub ready: bool,
}

/// Left in the data folder by "Update": the ODM the installer restarts shows its window even
/// when it was started with `--background` (the installer passes the old arguments on).
pub const SHOW_AFTER_UPDATE: &str = "show-after-update";

/// Whether ODM starts hidden in the tray: started with `--background`, unless an update just
/// restarted it.
pub fn starts_hidden<S: AsRef<str>>(args: impl IntoIterator<Item = S>, after_update: bool) -> bool {
    !after_update && args.into_iter().any(|a| a.as_ref() == "--background")
}

/// Reads and removes the "show after update" marker.
pub fn take_show_marker(data_dir: &Path) -> bool {
    std::fs::remove_file(data_dir.join(SHOW_AFTER_UPDATE)).is_ok()
}

fn leave_show_marker(data_dir: &Path) {
    if let Err(e) = std::fs::write(data_dir.join(SHOW_AFTER_UPDATE), b"") {
        tracing::warn!("could not leave the show-after-update marker: {e}");
    }
}

/// What a check's result does to the update found earlier (`found`) and the one already
/// downloaded (`ready`). `same` tells whether two updates are the same version.
pub fn adopt_check<T>(
    found: &mut Option<T>,
    ready: &mut Option<(T, Vec<u8>)>,
    latest: Option<T>,
    same: impl Fn(&T, &T) -> bool,
) {
    let Some(latest) = latest else {
        // No update any more (a pulled release): nothing may install at quit either.
        *found = None;
        *ready = None;
        return;
    };
    if !found.as_ref().is_some_and(|f| same(f, &latest)) {
        // A newer release replaces one downloaded earlier.
        *ready = None;
        *found = Some(latest);
    }
}

/// Update state shared by the commands, the timer and the quit flow.
pub struct Updates {
    supported: bool,
    endpoint: Option<Url>,
    memory_path: PathBuf,
    memory: Mutex<Memory>,
    found: Mutex<Option<Update>>,
    ready: Mutex<Option<(Update, Vec<u8>)>>,
    downloading: AtomicBool,
    check_lock: tokio::sync::Mutex<()>,
}

impl Updates {
    pub fn new(supported: bool, endpoint: Option<Url>, memory_path: PathBuf) -> Self {
        Self {
            supported,
            endpoint,
            memory: Mutex::new(Memory::load(&memory_path)),
            memory_path,
            found: Mutex::new(None),
            ready: Mutex::new(None),
            downloading: AtomicBool::new(false),
            check_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub fn supported(&self) -> bool {
        self.supported
    }

    fn remember(&self, f: impl FnOnce(&mut Memory)) {
        let mut m = self.memory.lock().unwrap();
        f(&mut m);
        if let Err(e) = m.save(&self.memory_path) {
            tracing::warn!("could not save the update state: {e}");
        }
    }

    fn status(&self, current: String) -> UpdateStatus {
        let available = self.found.lock().unwrap().as_ref().map(info_of);
        let memory = self.memory.lock().unwrap().clone();
        UpdateStatus {
            supported: self.supported,
            current,
            last_checked_ms: memory.last_checked_ms,
            dismissed: available
                .as_ref()
                .is_some_and(|a| memory.is_dismissed(&a.version)),
            available,
            downloading: self.downloading.load(Ordering::SeqCst),
            ready: self.ready.lock().unwrap().is_some(),
        }
    }
}

/// An updater for this check, with ODM's version rule, the endpoint override and the proxy
/// from the settings.
pub fn updater_for<R: Runtime>(
    app: &AppHandle<R>,
    settings: &Settings,
    endpoint: Option<&Url>,
) -> Result<Updater, Error> {
    let mut b = app
        .updater_builder()
        .version_comparator(|current, release| {
            is_newer(&current.to_string(), &release.version.to_string())
        });
    if let Some(url) = endpoint {
        b = b.endpoints(vec![url.clone()])?;
    }
    b = match settings.proxy_mode {
        ProxyMode::System => b,
        ProxyMode::None => b.no_proxy(),
        ProxyMode::Manual => match Url::parse(settings.proxy_url.trim()) {
            Ok(proxy) => b.proxy(proxy),
            Err(_) => b,
        },
    };
    b.build()
}

fn current_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

fn emit(app: &AppHandle, ev: &UiEvent) {
    if let Err(e) = app.emit(crate::EVENT, ev) {
        tracing::warn!("could not send an event to the window: {e}");
    }
}

fn settings_now(app: &AppHandle) -> Settings {
    app.state::<AppState>().settings.lock().unwrap().clone()
}

/// The automatic checks: 10 s after start, then every 6 h, while "Check for updates
/// automatically" is on. Only started in a build with a key.
pub async fn run(app: AppHandle) {
    let mut n = 0;
    loop {
        tokio::time::sleep(check_delay(n)).await;
        n = n.saturating_add(1);
        let supported = app.state::<Updates>().supported();
        if should_check(supported, settings_now(&app).update_check, false) {
            if let Err(e) = check(&app, false).await {
                tracing::info!("automatic update check: {e}");
            }
        }
    }
}

/// Looks for an update. A new one is announced (banner, and one notification per version);
/// with "Install when I quit ODM" it starts downloading in the background.
pub async fn check(app: &AppHandle, manual: bool) -> Result<UpdateStatus, String> {
    let up = app.state::<Updates>();
    if !should_check(up.supported, true, true) {
        return Err(UNAVAILABLE.into());
    }
    let _one_at_a_time = up.check_lock.lock().await;
    let settings = settings_now(app);
    let found = match updater_for(app, &settings, up.endpoint.as_ref()) {
        Ok(updater) => match tokio::time::timeout(CHECK_TIMEOUT, updater.check()).await {
            Ok(r) => r,
            Err(_) => Err(Error::Network("the update check timed out".into())),
        },
        Err(e) => Err(e),
    };
    let found = match found {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("update check failed: {e}");
            return Err(user_error(Stage::Check, &e).into());
        }
    };
    up.remember(|m| m.last_checked_ms = Some(odm_appcore::ipc::now_ms()));
    let info = found.as_ref().map(info_of);
    adopt_check(
        &mut up.found.lock().unwrap(),
        &mut up.ready.lock().unwrap(),
        found,
        |a, b| a.version == b.version,
    );
    let Some(info) = info else {
        return Ok(up.status(current_version(app)));
    };
    tracing::info!("ODM {} is available", info.version);
    if manual {
        // Asking again brings back a banner that "Later" hid.
        up.remember(|m| m.dismissed = None);
    }
    let mut first = false;
    up.remember(|m| first = m.take_notify(&info.version));
    if first {
        let title = format!("ODM {} is available", info.version);
        if let Err(e) = app
            .notification()
            .builder()
            .title(title)
            .body("Open ODM to see what's new.")
            .show()
        {
            tracing::debug!("notification failed: {e}");
        }
    }
    let dismissed = up.memory.lock().unwrap().is_dismissed(&info.version);
    if !dismissed {
        emit(
            app,
            &UiEvent::UpdateAvailable {
                version: info.version.clone(),
                notes: info.notes.clone(),
                date: info.date.clone(),
            },
        );
    }
    let ready = up.ready.lock().unwrap().is_some();
    if settings.update_install == UpdateInstall::Auto && !ready {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = download(&app).await {
                tracing::info!("background update download: {e}");
            }
        });
    }
    Ok(up.status(current_version(app)))
}

/// Downloads the update found by the last check and verifies its signature.
pub async fn download(app: &AppHandle) -> Result<(), String> {
    let up = app.state::<Updates>();
    if up.downloading.swap(true, Ordering::SeqCst) {
        return Err("The update is already downloading.".into());
    }
    let Some(update) = up.found.lock().unwrap().clone() else {
        up.downloading.store(false, Ordering::SeqCst);
        return Err("There is no update to download.".into());
    };
    emit(app, &UiEvent::UpdateProgress { percent: Some(0) });
    let progress_app = app.clone();
    let mut got: u64 = 0;
    let mut last: Option<u8> = Some(0);
    let r = update
        .download(
            move |n, total| {
                got += n as u64;
                let percent = total
                    .filter(|t| *t > 0)
                    .map(|t| (got.saturating_mul(100) / t).min(100) as u8);
                if percent != last {
                    last = percent;
                    emit(&progress_app, &UiEvent::UpdateProgress { percent });
                }
            },
            || {},
        )
        .await;
    up.downloading.store(false, Ordering::SeqCst);
    match r {
        Ok(bytes) => {
            let version = update.version.clone();
            tracing::info!("update {version} downloaded and verified");
            *up.ready.lock().unwrap() = Some((update, bytes));
            emit(app, &UiEvent::UpdateReady { version });
            Ok(())
        }
        Err(e) => {
            tracing::warn!("update download failed: {e}");
            let message = user_error(Stage::Download, &e).to_string();
            emit(
                app,
                &UiEvent::UpdateFailed {
                    message: message.clone(),
                },
            );
            Err(message)
        }
    }
}

/// Stops the downloads the way quitting does: their current writes finish, they are saved as
/// paused-for-restart, and the browser pipe closes.
async fn stop_for_install(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.quitting.store(true, Ordering::SeqCst);
    state.ipc_stop.cancel();
    state.manager.shutdown().await;
}

/// "Update" in the banner: downloads if needed, stops the downloads, then runs the installer
/// (passive) which restarts ODM. Only returns on failure.
pub async fn install_now(app: &AppHandle) -> Result<(), String> {
    let up = app.state::<Updates>();
    if up.ready.lock().unwrap().is_none() {
        download(app).await?;
    }
    let Some((update, bytes)) = up.ready.lock().unwrap().take() else {
        return Err("There is no update to install.".into());
    };
    stop_for_install(app).await;
    // The restarted ODM (and `app.restart()` below) must come back with its window.
    leave_show_marker(&app.state::<AppState>().paths.data_dir);
    let r = tauri::async_runtime::spawn_blocking(move || update.install(bytes)).await;
    // `install` exits ODM once the installer runs; reaching this line means it did not.
    match r {
        Ok(Err(e)) => tracing::error!("update install failed: {e}"),
        Ok(Ok(())) => tracing::error!("update install returned without exiting"),
        Err(e) => tracing::error!("update install task failed: {e}"),
    }
    let dialog_app = app.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .message(INSTALL_FAILED)
            .title("ODM")
            .kind(MessageDialogKind::Error)
            .blocking_show()
    })
    .await;
    // The engine is stopped; a fresh start resumes the downloads.
    app.restart();
}

/// Called by the quit flow after the downloads stopped: with "Install when I quit ODM" and an
/// update downloaded, runs the installer without restarting ODM. Returns when there is nothing
/// to install or the installer could not start.
pub async fn install_on_quit(app: &AppHandle) {
    let Some(up) = app.try_state::<Updates>() else {
        return;
    };
    if settings_now(app).update_install != UpdateInstall::Auto {
        return;
    }
    let Some((update, bytes)) = up.ready.lock().unwrap().take() else {
        return;
    };
    app.state::<AppState>().ipc_stop.cancel();
    let update = update.restart_after_install(false);
    match tauri::async_runtime::spawn_blocking(move || update.install(bytes)).await {
        Ok(Err(e)) => tracing::error!("update install at quit failed: {e}"),
        Ok(Ok(())) => {}
        Err(e) => tracing::error!("update install task failed: {e}"),
    }
}

type Res<T> = Result<T, String>;

#[tauri::command]
pub fn update_status(app: AppHandle) -> UpdateStatus {
    app.state::<Updates>().status(current_version(&app))
}

/// "Check now" in Settings → Updates.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> Res<UpdateStatus> {
    check(&app, true).await
}

/// "Later": hides the banner until a newer version is out.
#[tauri::command]
pub fn update_dismiss(app: AppHandle, version: String) {
    app.state::<Updates>().remember(|m| m.dismiss(&version));
}

/// "Update" in the banner.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Res<()> {
    install_now(&app).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEST_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDVGMEYzMTBCNTk5RjM4RDEKUldUUk9KOVpDekVQWDFKblZYc0dNeWZ5Ylg4ZVlYREtCZ2dFTEZlQjZMWG1zcHl0WVdUdHpUQzIK";

    #[test]
    fn an_update_restart_is_never_hidden() {
        assert!(starts_hidden(["ODM.exe", "--background"], false));
        assert!(!starts_hidden(["ODM.exe", "--background"], true));
        assert!(!starts_hidden(["ODM.exe"], false));
        assert!(!starts_hidden(["ODM.exe", "--backgroundx"], false));
    }

    #[test]
    fn the_show_marker_is_read_once() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!take_show_marker(dir.path()));
        leave_show_marker(dir.path());
        assert!(take_show_marker(dir.path()));
        assert!(!take_show_marker(dir.path()), "removed by the first read");
    }

    #[test]
    fn a_pulled_release_is_not_installed() {
        let mut found = Some("0.2.0");
        let mut ready = Some(("0.2.0", vec![1u8]));
        // The same version again keeps the download.
        adopt_check(&mut found, &mut ready, Some("0.2.0"), |a, b| a == b);
        assert_eq!((found, ready.is_some()), (Some("0.2.0"), true));
        // A newer one replaces it.
        adopt_check(&mut found, &mut ready, Some("0.2.1"), |a, b| a == b);
        assert_eq!((found, ready.is_some()), (Some("0.2.1"), false));
        ready = Some(("0.2.1", vec![1]));
        // No update any more (the release was pulled): nothing waits for the quit.
        adopt_check(&mut found, &mut ready, None, |a, b| a == b);
        assert_eq!((found, ready.is_some()), (None, false));
    }

    #[test]
    fn versions_compare_as_semver() {
        assert!(is_newer("0.1.0", "0.2.0"));
        assert!(is_newer("0.1.0", "0.1.1"));
        assert!(is_newer("0.9.0", "0.10.0"));
        assert!(is_newer("0.1.0", "v0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("0.1.0", "0.2.0-beta.1"));
        assert!(!is_newer("0.2.0", "0.2.0-beta.1"));
        assert!(!is_newer("0.1.0", "latest"));
        assert!(!is_newer("garbage", "0.2.0"));
    }

    #[test]
    fn each_version_is_announced_once() {
        let mut m = Memory::default();
        assert!(m.take_notify("0.2.0"));
        assert!(!m.take_notify("0.2.0"));
        assert!(m.take_notify("0.3.0"));
        assert!(!m.take_notify("0.3.0"));
    }

    #[test]
    fn later_hides_only_that_version() {
        let mut m = Memory::default();
        assert!(!m.is_dismissed("0.2.0"));
        m.dismiss("0.2.0");
        assert!(m.is_dismissed("0.2.0"));
        assert!(!m.is_dismissed("0.3.0"));
    }

    #[test]
    fn memory_survives_a_restart_and_a_damaged_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("updates.json");
        assert_eq!(Memory::load(&path), Memory::default());
        let mut m = Memory::default();
        m.take_notify("0.2.0");
        m.dismiss("0.2.0");
        m.last_checked_ms = Some(42);
        m.save(&path).unwrap();
        assert_eq!(Memory::load(&path), m);
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(Memory::load(&path), Memory::default());
    }

    #[test]
    fn checks_10_seconds_after_start_then_every_6_hours() {
        assert_eq!(check_delay(0), Duration::from_secs(10));
        assert_eq!(check_delay(1), Duration::from_secs(6 * 3600));
        assert_eq!(check_delay(7), Duration::from_secs(6 * 3600));
    }

    #[test]
    fn no_key_never_checks() {
        for update_check in [true, false] {
            for manual in [true, false] {
                assert!(!should_check(false, update_check, manual));
            }
        }
        assert!(should_check(true, true, false));
        assert!(!should_check(true, false, false));
        assert!(should_check(true, false, true));
    }

    #[test]
    fn the_key_comes_from_the_compiled_config() {
        let mut plugins = HashMap::new();
        assert_eq!(pubkey_in(&plugins), "");
        assert!(!has_pubkey(&pubkey_in(&plugins)));
        plugins.insert(
            "updater".to_string(),
            json!({ "pubkey": "", "endpoints": [] }),
        );
        assert!(!has_pubkey(&pubkey_in(&plugins)));
        plugins.insert("updater".to_string(), json!({ "pubkey": "  " }));
        assert!(!has_pubkey(&pubkey_in(&plugins)));
        plugins.insert("updater".to_string(), json!({ "pubkey": TEST_PUBKEY }));
        assert_eq!(pubkey_in(&plugins), TEST_PUBKEY);
        assert!(has_pubkey(&pubkey_in(&plugins)));
    }

    #[test]
    fn endpoint_override_takes_https_or_http_on_this_pc() {
        assert_eq!(endpoint_override(None), None);
        assert_eq!(endpoint_override(Some("  ")), None);
        assert_eq!(endpoint_override(Some("not a url")), None);
        let https = endpoint_override(Some("https://example.com/latest.json")).unwrap();
        assert_eq!(https.url.as_str(), "https://example.com/latest.json");
        assert!(!https.loopback_http);
        for local in [
            "http://127.0.0.1:8080/latest.json",
            "http://localhost:8080/latest.json",
            "http://[::1]:8080/latest.json",
        ] {
            assert!(
                endpoint_override(Some(local)).unwrap().loopback_http,
                "{local}"
            );
        }
        assert_eq!(
            endpoint_override(Some("http://example.com/latest.json")),
            None
        );
        assert_eq!(endpoint_override(Some("file:///C:/latest.json")), None);
    }

    #[test]
    fn insecure_transport_only_for_a_loopback_override() {
        let base = json!({ "pubkey": TEST_PUBKEY, "endpoints": [] });
        let mut plugins = HashMap::from([("updater".to_string(), base.clone())]);
        assert_eq!(prepare_config(&mut plugins, None), TEST_PUBKEY);
        assert_eq!(plugins["updater"], base);
        let https = endpoint_override(Some("https://example.com/l.json"));
        prepare_config(&mut plugins, https.as_ref());
        assert_eq!(plugins["updater"], base);
        let local = endpoint_override(Some("http://127.0.0.1:1/l.json"));
        assert_eq!(prepare_config(&mut plugins, local.as_ref()), TEST_PUBKEY);
        assert_eq!(
            plugins["updater"]["dangerousInsecureTransportProtocol"],
            json!(true)
        );
        assert_eq!(plugins["updater"]["pubkey"], json!(TEST_PUBKEY));
    }

    #[test]
    fn errors_read_as_plain_sentences() {
        let net = Error::Network("boom".into());
        assert_eq!(user_error(Stage::Check, &net), CHECK_FAILED);
        assert_eq!(user_error(Stage::Download, &net), DOWNLOAD_FAILED);
        assert_eq!(user_error(Stage::Install, &net), INSTALL_FAILED);
        let bad = Error::SignatureUtf8("x".into());
        assert_eq!(user_error(Stage::Download, &bad), VERIFY_FAILED);
        let mismatch = Error::SignedVersionMismatch {
            signed: "0.1.0".into(),
            announced: "0.2.0".into(),
        };
        assert_eq!(user_error(Stage::Download, &mismatch), VERIFY_FAILED);
    }
}
