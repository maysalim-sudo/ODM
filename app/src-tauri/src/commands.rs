use odm_appcore::api::{self, AddArgs, BatchResult, ConfirmArgs, Suggestion};
use odm_appcore::dto::{DetailDto, DownloadDto, UiEvent};
use odm_appcore::integration::{self, IntegrationStatus};
use odm_appcore::offers::OfferDto;
use odm_appcore::tools::{self, ToolSources, ToolsStatus};
use odm_appcore::Settings;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::AppState;

type Res<T> = Result<T, String>;

fn msg(e: odm_core::OdmError) -> String {
    api::user_message(&e)
}

#[tauri::command]
pub async fn list_downloads(state: State<'_, AppState>) -> Res<Vec<DownloadDto>> {
    api::list(&state.manager).await
}

#[tauri::command]
pub async fn get_detail(state: State<'_, AppState>, id: String) -> Res<Option<DetailDto>> {
    api::detail(&state.manager, &id).await
}

fn starts_paused(state: &AppState) -> bool {
    api::starts_paused(&state.settings.lock().unwrap())
}

#[tauri::command]
pub async fn add_download(state: State<'_, AppState>, args: AddArgs) -> Res<String> {
    let args = api::with_start_mode(&state.settings.lock().unwrap(), args);
    api::add(&state.manager, args).await
}

#[tauri::command]
pub async fn add_batch(
    state: State<'_, AppState>,
    text: String,
    save_dir: Option<String>,
) -> Res<BatchResult> {
    let paused = starts_paused(&state);
    api::add_batch(&state.manager, &text, save_dir, paused).await
}

/// The answer to "<name> already exists": replace, keep_both or cancel.
#[tauri::command]
pub async fn answer_exists(
    state: State<'_, AppState>,
    id: String,
    choice: odm_engine::ExistsChoice,
) -> Res<()> {
    api::answer_exists(&state.manager, &id, choice).await
}

/// A download's own speed limit in KB/s; null removes it.
#[tauri::command]
pub async fn set_download_limit(
    state: State<'_, AppState>,
    id: String,
    kbps: Option<u64>,
) -> Res<()> {
    api::set_download_limit(&state.manager, &id, kbps).await
}

/// Cancels the countdown "When the queue is done" started (sleep, shut down or quit).
#[tauri::command]
pub async fn cancel_after_queue(app: AppHandle, state: State<'_, AppState>) -> Res<()> {
    // `shutdown /a` runs a program; keep it off the async workers.
    let power = state.power.clone();
    let ev = tauri::async_runtime::spawn_blocking(move || power.cancel_pending())
        .await
        .map_err(|e| e.to_string())?;
    if let Some(ev) = ev {
        if let Err(e) = app.emit(crate::EVENT, &ev) {
            tracing::warn!("could not send an event to the window: {e}");
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn suggest(state: State<'_, AppState>, url: String) -> Res<Suggestion> {
    // The guard is a temporary: it is dropped before the await.
    let cfg = state.settings.lock().unwrap().to_engine_config();
    api::suggest(&url, &cfg).await
}

#[tauri::command]
pub async fn pause(state: State<'_, AppState>, ids: Vec<String>) -> Res<()> {
    for id in ids {
        state.manager.pause(&id).await.map_err(msg)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn resume(state: State<'_, AppState>, ids: Vec<String>) -> Res<()> {
    for id in ids {
        state.manager.resume(&id).await.map_err(msg)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel(state: State<'_, AppState>, ids: Vec<String>, delete_file: bool) -> Res<()> {
    for id in ids {
        state.manager.cancel(&id, delete_file).await.map_err(msg)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn remove(state: State<'_, AppState>, ids: Vec<String>, delete_file: bool) -> Res<()> {
    for id in ids {
        state.manager.remove(&id, delete_file).await.map_err(msg)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn pause_all(state: State<'_, AppState>) -> Res<()> {
    state.manager.pause_all().await.map_err(msg)
}

#[tauri::command]
pub async fn resume_all(state: State<'_, AppState>) -> Res<()> {
    state.manager.resume_all().await.map_err(msg)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

/// "Start ODM with Windows" through `tauri-plugin-autostart`: an HKCU Run entry that starts
/// ODM with `--background`, so it opens in the tray.
struct PluginAutostart<'a>(&'a AppHandle);

impl odm_appcore::autostart::Autostart for PluginAutostart<'_> {
    fn set_enabled(&self, on: bool) -> Result<(), String> {
        use tauri_plugin_autostart::ManagerExt;
        let a = self.0.autolaunch();
        if on { a.enable() } else { a.disable() }.map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Res<Settings> {
    apply_settings(&app, &state, settings).await
}

/// Saves settings and applies them; the Settings page and the tray both come through here.
/// Afterwards the window hears about the result (`odm://settings`) and the tray's speed menu
/// shows the limit now in force.
pub async fn apply_settings(
    app: &AppHandle,
    state: &AppState,
    settings: Settings,
) -> Res<Settings> {
    // One save at a time: each reads the settings the previous one stored.
    let _one_at_a_time = state.settings_save.lock().await;
    let prev = state.settings.lock().unwrap().clone();
    let (saved, warnings) = api::update_settings(
        &state.manager,
        &state.paths.settings,
        &prev,
        settings,
        &PluginAutostart(app),
    )
    .await?;
    for w in warnings {
        tracing::warn!("settings: {w}");
    }
    *state.settings.lock().unwrap() = saved.clone();
    // "Prevent sleep" applies at once, not at the next download change.
    let active = state.manager.active_count().await.unwrap_or(0);
    state.power.set_activity(active, saved.prevent_sleep);
    crate::shell::sync_speed_menu(app, saved.speed_limit_kbps);
    if let Err(e) = app.emit(crate::SETTINGS_EVENT, &saved) {
        tracing::warn!("could not send the settings to the window: {e}");
    }
    Ok(saved)
}

async fn file_path(state: &State<'_, AppState>, id: &str) -> Res<(PathBuf, PathBuf)> {
    let (rec, _) = state
        .manager
        .get(id)
        .await
        .map_err(msg)?
        .ok_or("That download no longer exists.")?;
    let dir = PathBuf::from(&rec.save_dir);
    Ok((dir.join(&rec.filename), dir))
}

#[tauri::command]
pub async fn open_file(app: AppHandle, state: State<'_, AppState>, id: String) -> Res<()> {
    let (file, _) = file_path(&state, &id).await?;
    if !file.exists() {
        return Err("The file is not there. It may have been moved or deleted.".into());
    }
    app.opener()
        .open_path(file.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_folder(app: AppHandle, state: State<'_, AppState>, id: String) -> Res<()> {
    let (file, dir) = file_path(&state, &id).await?;
    if file.exists() {
        app.opener()
            .reveal_item_in_dir(&file)
            .map_err(|e| e.to_string())
    } else if dir.exists() {
        app.opener()
            .open_path(dir.to_string_lossy(), None::<&str>)
            .map_err(|e| e.to_string())
    } else {
        Err("The folder is not there yet.".into())
    }
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle, start: Option<String>) -> Res<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app.dialog().file();
        if let Some(s) = start.filter(|s| !s.trim().is_empty()) {
            dialog = dialog.set_directory(s);
        }
        dialog
            .blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())
}

/// Picks a program (`.exe`), for the FFmpeg and yt-dlp locations.
#[tauri::command]
pub async fn pick_program(app: AppHandle, start: Option<String>) -> Res<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app.dialog().file().add_filter("Programs", &["exe"]);
        if let Some(dir) = start
            .filter(|s| !s.trim().is_empty())
            .and_then(|s| PathBuf::from(s).parent().map(PathBuf::from))
        {
            dialog = dialog.set_directory(dir);
        }
        dialog
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())
}

/// The settings a new install starts with, for "Restore defaults".
#[tauri::command]
pub fn default_settings() -> Settings {
    Settings::default()
}

/// Opens ODM's own folders: `"logs"` or `"data"`.
#[tauri::command]
pub fn open_app_folder(app: AppHandle, state: State<'_, AppState>, kind: String) -> Res<()> {
    let dir = match kind.as_str() {
        "logs" => &state.paths.logs,
        "data" => &state.paths.data_dir,
        _ => return Err("Unknown folder.".into()),
    };
    if let Err(e) = std::fs::create_dir_all(dir) {
        tracing::warn!("could not create {}: {e}", dir.display());
    }
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_offers(state: State<'_, AppState>) -> Vec<OfferDto> {
    state.offers.list()
}

#[tauri::command]
pub async fn resolve_offer(
    state: State<'_, AppState>,
    offer_id: String,
    reuse: bool,
) -> Res<String> {
    api::resolve_offer(&state.manager, &state.offers, &offer_id, reuse).await
}

#[tauri::command]
pub fn list_pending_confirms(state: State<'_, AppState>) -> Vec<String> {
    state.pending.list()
}

#[tauri::command]
pub async fn confirm_download(state: State<'_, AppState>, args: ConfirmArgs) -> Res<()> {
    api::confirm_download(&state.manager, &state.pending, args).await
}

#[tauri::command]
pub async fn refresh_url(state: State<'_, AppState>, id: String, url: String) -> Res<()> {
    api::refresh_url(&state.manager, &id, &url).await
}

/// ODM and the browser helper that sits next to it.
fn exe_paths() -> Res<(PathBuf, PathBuf)> {
    let app = std::env::current_exe().map_err(|_| "ODM could not find its own location.")?;
    let host = app.with_file_name("odm-host.exe");
    Ok((app, host))
}

fn current_status(state: &AppState, host: &std::path::Path) -> IntegrationStatus {
    integration::status(
        host,
        state.pipe_running.load(Ordering::SeqCst),
        &state.contact,
    )
}

#[tauri::command]
pub fn integration_status(state: State<'_, AppState>) -> Res<IntegrationStatus> {
    let (_, host) = exe_paths()?;
    Ok(current_status(&state, &host))
}

#[tauri::command]
pub async fn integration_test() -> Res<String> {
    let (_, host) = exe_paths()?;
    tauri::async_runtime::spawn_blocking(move || integration::self_test(&host))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn integration_repair(state: State<'_, AppState>) -> Res<IntegrationStatus> {
    let (app, host) = exe_paths()?;
    let h = host.clone();
    tauri::async_runtime::spawn_blocking(move || integration::repair(&h, &app))
        .await
        .map_err(|e| e.to_string())??;
    Ok(current_status(&state, &host))
}

#[tauri::command]
pub fn open_extension_folder(app: AppHandle, kind: String) -> Res<()> {
    let sub = match kind.as_str() {
        "chromium" | "firefox" => kind.as_str(),
        _ => return Err("Unknown browser type.".into()),
    };
    let dir = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("extension")
        .join(sub);
    if !dir.is_dir() {
        return Err("The extension files are not included in this copy of ODM.".into());
    }
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Opens the browser on its extensions page (`chrome://extensions` and the like).
#[tauri::command]
pub async fn open_browser_extensions_page(browser: String) -> Res<()> {
    tauri::async_runtime::spawn_blocking(move || {
        odm_appcore::browsers::open_extensions_page(&browser)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens a store listing (Chrome Web Store, Edge Add-ons, AMO) in the chosen browser.
#[tauri::command]
pub async fn open_store_page(browser: String, url: String) -> Res<()> {
    tauri::async_runtime::spawn_blocking(move || {
        odm_appcore::browsers::open_store_page(&browser, &url)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Store listings of the extension; null entries are not published yet.
#[tauri::command]
pub fn store_links() -> odm_core::ids::StoreLinks {
    odm_core::ids::store_links()
}

#[tauri::command]
pub async fn tools_status(state: State<'_, AppState>) -> Res<ToolsStatus> {
    let cfg = state.settings.lock().unwrap().to_engine_config();
    tauri::async_runtime::spawn_blocking(move || tools::status(&cfg))
        .await
        .map_err(|e| e.to_string())
}

/// Downloads FFmpeg, yt-dlp and Deno into ODM's tools folder, reporting progress as
/// `UiEvent::ToolsProgress`. Resolves with the versions found afterwards.
#[tauri::command]
pub async fn tools_install(app: AppHandle, state: State<'_, AppState>) -> Res<ToolsStatus> {
    if state.tools_busy.swap(true, Ordering::SeqCst) {
        return Err("The video tools are already being downloaded.".into());
    }
    let dir = state.paths.tools_dir.clone();
    let cfg = state.settings.lock().unwrap().to_engine_config();
    let proxy = cfg.proxy.clone();
    let progress_app = app.clone();
    let installed = tauri::async_runtime::spawn(async move {
        let on_progress = move |stage: &str, done: u64, total: u64| {
            let ev = UiEvent::ToolsProgress {
                stage: stage.to_string(),
                done,
                total,
            };
            if let Err(e) = progress_app.emit(crate::EVENT, &ev) {
                tracing::warn!("could not send tools progress to the window: {e}");
            }
        };
        let result = tools::install(
            &dir,
            &tools::http_client(&proxy),
            &ToolSources::default(),
            &on_progress,
        )
        .await;
        match result {
            Ok(()) => tauri::async_runtime::spawn_blocking(move || tools::status(&cfg))
                .await
                .map_err(|e| e.to_string()),
            Err(e) => Err(e),
        }
    })
    .await;
    state.tools_busy.store(false, Ordering::SeqCst);
    match installed {
        Ok(r) => r,
        Err(e) => Err(e.to_string()),
    }
}
