mod commands;

use odm_appcore::api::user_message;
use odm_appcore::dto::UiEvent;
use odm_appcore::{bridge, logging, AppPaths, Settings};
use odm_engine::ManagerHandle;
use odm_store::Store;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager, RunEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

pub const EVENT: &str = "odm://event";
/// Progress reaches the UI at most 4 times per second.
const UI_PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

pub struct AppState {
    pub manager: ManagerHandle,
    pub settings: Mutex<Settings>,
    pub paths: AppPaths,
}

fn start_engine(paths: &AppPaths, settings: &Settings) -> Result<ManagerHandle, String> {
    let store =
        Store::open(&paths.db).map_err(|e| format!("ODM could not open its database.\n\n{e}"))?;
    tauri::async_runtime::block_on(ManagerHandle::start(settings.to_engine_config(), store))
        .map_err(|e| {
            format!(
                "ODM could not start its download engine.\n\n{}",
                user_message(&e)
            )
        })
}

pub fn run() {
    let paths = AppPaths::from_env();
    let _ = paths.ensure();
    // Kept alive until the process exits, so buffered log lines are flushed.
    let _log_guard = logging::init(&paths.logs).ok();
    let (settings, warnings) = Settings::load(&paths.settings);
    for w in &warnings {
        tracing::warn!("settings: {w}");
    }
    tracing::info!(data_dir = %paths.data_dir.display(), "ODM starting");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let manager = match start_engine(&paths, &settings) {
                Ok(m) => m,
                Err(message) => {
                    tracing::error!("{message}");
                    app.dialog()
                        .message(&message)
                        .title("ODM")
                        .kind(MessageDialogKind::Error)
                        .blocking_show();
                    return Err(message.into());
                }
            };
            app.manage(AppState {
                manager: manager.clone(),
                settings: Mutex::new(settings),
                paths,
            });

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(bridge::run(
                manager,
                UI_PROGRESS_INTERVAL,
                move |ev: UiEvent| {
                    if let Err(e) = handle.emit(EVENT, &ev) {
                        tracing::warn!("could not send an event to the window: {e}");
                    }
                },
            ));

            if let Some(w) = app.get_webview_window("main") {
                w.show()?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_downloads,
            commands::get_detail,
            commands::add_download,
            commands::add_batch,
            commands::suggest,
            commands::pause,
            commands::resume,
            commands::cancel,
            commands::remove,
            commands::pause_all,
            commands::resume_all,
            commands::get_settings,
            commands::save_settings,
            commands::open_file,
            commands::open_folder,
            commands::pick_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ODM");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            if let Some(state) = app.try_state::<AppState>() {
                // Stops downloads cleanly; they are left queued and resume next start.
                tauri::async_runtime::block_on(state.manager.shutdown());
            }
        }
    });
}
