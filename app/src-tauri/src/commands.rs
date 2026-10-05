use odm_appcore::api::{self, AddArgs, BatchResult, ConfirmArgs, Suggestion};
use odm_appcore::dto::{DetailDto, DownloadDto};
use odm_appcore::offers::OfferDto;
use odm_appcore::Settings;
use std::path::PathBuf;
use tauri::{AppHandle, State};
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

#[tauri::command]
pub async fn add_download(state: State<'_, AppState>, args: AddArgs) -> Res<String> {
    api::add(&state.manager, args).await
}

#[tauri::command]
pub async fn add_batch(
    state: State<'_, AppState>,
    text: String,
    save_dir: Option<String>,
) -> Res<BatchResult> {
    api::add_batch(&state.manager, &text, save_dir).await
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

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: Settings) -> Res<Settings> {
    let (saved, warnings) =
        api::save_settings(&state.manager, &state.paths.settings, settings).await?;
    for w in warnings {
        tracing::warn!("settings: {w}");
    }
    *state.settings.lock().unwrap() = saved.clone();
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
