mod commands;
mod shell;

use odm_appcore::api::{store_open_message, user_message};
use odm_appcore::confirm::Pending;
use odm_appcore::dto::{CompletionTracker, UiEvent};
use odm_appcore::ipc::IpcContext;
use odm_appcore::offers::Offers;
#[cfg(windows)]
use odm_appcore::pipe;
use odm_appcore::{bridge, logging, registration, summary, AppPaths, Settings};
use odm_engine::ManagerHandle;
use odm_store::Store;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter, Manager, RunEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tokio_util::sync::CancellationToken;

pub const EVENT: &str = "odm://event";
/// Progress reaches the UI at most 4 times per second.
const UI_PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

pub struct AppState {
    pub manager: ManagerHandle,
    pub settings: Arc<Mutex<Settings>>,
    pub paths: AppPaths,
    /// Browser downloads that match a paused or failed one, waiting for the user's choice.
    pub offers: Arc<Offers>,
    /// Browser downloads added paused that wait for the "New download" dialog.
    pub pending: Arc<Pending>,
    /// Cancelled on exit to stop the browser-integration pipe server.
    pub ipc_stop: CancellationToken,
    /// Downloads already announced with a notification.
    pub completions: Mutex<CompletionTracker>,
    /// Set while the quit flow runs, so it cannot start twice.
    pub quitting: AtomicBool,
}

fn start_engine(paths: &AppPaths, settings: &Settings) -> Result<ManagerHandle, String> {
    let store = Store::open(&paths.db).map_err(|e| {
        // Raw detail goes to the log only; it has no URLs or header values.
        tracing::error!("could not open the database: {e}");
        store_open_message(&e)
    })?;
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
    let mut log_guard = logging::init(&paths.logs).ok();
    let (settings, warnings) = Settings::load(&paths.settings);
    for w in &warnings {
        tracing::warn!("settings: {w}");
    }
    tracing::info!(data_dir = %paths.data_dir.display(), "ODM starting");
    // The browser host starts ODM with --background so no window pops up.
    let background = std::env::args().any(|a| a == "--background");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if !argv.iter().any(|a| a == "--background") {
                shell::show_main(app);
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
                    // Flush the log, then leave without Tauri's setup-failure panic.
                    drop(log_guard.take());
                    std::process::exit(1);
                }
            };
            let settings = Arc::new(Mutex::new(settings));
            let offers = Arc::new(Offers::default());
            let pending = Arc::new(Pending::default());
            let ipc_stop = CancellationToken::new();
            app.manage(AppState {
                manager: manager.clone(),
                settings: settings.clone(),
                paths,
                offers: offers.clone(),
                pending: pending.clone(),
                ipc_stop: ipc_stop.clone(),
                completions: Mutex::new(CompletionTracker::default()),
                quitting: AtomicBool::new(false),
            });
            // Keep the log flushing for as long as the app runs.
            app.manage(Mutex::new(log_guard));

            let handle = app.handle().clone();
            // `track` spawns a task, so it needs the async runtime context.
            let speeds = tauri::async_runtime::block_on(async { summary::track(&manager) });
            let show_handle = handle.clone();
            let offer_handle = handle.clone();
            let confirm_handle = handle.clone();
            let ctx = Arc::new(IpcContext {
                manager: manager.clone(),
                settings,
                speeds,
                offers,
                show_window: Box::new(move || {
                    let h = show_handle.clone();
                    if let Err(e) = show_handle.run_on_main_thread(move || shell::show_main(&h)) {
                        tracing::warn!("could not show the window: {e}");
                    }
                }),
                on_offer: Box::new(move |offer| {
                    let ev = UiEvent::ReuseOffer {
                        offer: offer.clone(),
                    };
                    if let Err(e) = offer_handle.emit(EVENT, &ev) {
                        tracing::warn!("could not send an offer to the window: {e}");
                    }
                }),
                pending,
                on_confirm: Box::new(move |id| {
                    let ev = UiEvent::ConfirmDownload { id: id.to_string() };
                    if let Err(e) = confirm_handle.emit(EVENT, &ev) {
                        tracing::warn!("could not send a confirmation to the window: {e}");
                    }
                }),
            });
            match tauri::async_runtime::block_on(pipe::serve(
                pipe::pipe_name_from_env(),
                ctx,
                ipc_stop,
            )) {
                Ok(_) => {}
                Err(e) => tracing::error!("browser integration is unavailable: {e}"),
            }
            if let Ok(app_exe) = std::env::current_exe() {
                let host_exe = app_exe.with_file_name("odm-host.exe");
                std::thread::spawn(move || registration::ensure_registered(&host_exe, &app_exe));
            }

            tauri::async_runtime::spawn(bridge::run(
                manager,
                UI_PROGRESS_INTERVAL,
                move |ev: UiEvent| {
                    shell::on_ui_event(&handle, &ev);
                    if let Err(e) = handle.emit(EVENT, &ev) {
                        tracing::warn!("could not send an event to the window: {e}");
                    }
                },
            ));

            shell::setup(app, background)?;
            Ok(())
        })
        .on_window_event(shell::on_window_event)
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
            commands::list_offers,
            commands::resolve_offer,
            commands::list_pending_confirms,
            commands::confirm_download,
            commands::refresh_url,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ODM");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            if let Some(state) = app.try_state::<AppState>() {
                state.ipc_stop.cancel();
                // Stops downloads cleanly; they are left queued and resume next start.
                tauri::async_runtime::block_on(state.manager.shutdown());
            }
        }
    });
}
