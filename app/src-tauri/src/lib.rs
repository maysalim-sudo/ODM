mod commands;
mod shell;
mod speed_menu;
pub mod torrent_arg;
pub mod updates;

use odm_appcore::api::{store_open_message, user_message};
use odm_appcore::confirm::Pending;
use odm_appcore::dto::{CompletionTracker, UiEvent};
use odm_appcore::integration::Contact;
use odm_appcore::ipc::IpcContext;
use odm_appcore::offers::Offers;
use odm_appcore::pipe;
use odm_appcore::power::{Power, SystemPower};
use odm_appcore::{bridge, logging, registration, summary, AppPaths, Settings};
use odm_engine::ManagerHandle;
use odm_store::Store;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter, Manager, RunEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tokio_util::sync::CancellationToken;

pub const EVENT: &str = "odm://event";
/// Sent with the saved settings after every save, so the chip and the Settings page stay in step.
pub const SETTINGS_EVENT: &str = "odm://settings";
/// Asks the window to open the Settings page; the payload is the section id.
pub const OPEN_SETTINGS_EVENT: &str = "odm://open-settings";
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
    /// The last request a browser sent, shared with the IPC dispatcher.
    pub contact: Arc<Contact>,
    /// True once the browser-integration pipe is bound.
    pub pipe_running: AtomicBool,
    /// True while "Get video tools" is downloading, so it cannot start twice.
    pub tools_busy: AtomicBool,
    /// Prevent sleep and the "When the queue is done" action.
    pub power: Arc<Power<shell::ShellPower>>,
    /// Held while settings are saved and applied, so saves from the Settings page, the
    /// speed chip and the tray run one after another.
    pub settings_save: tokio::sync::Mutex<()>,
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
    let (mut settings, warnings) = Settings::load(&paths.settings);
    for w in &warnings {
        tracing::warn!("settings: {w}");
    }
    // The torrent listen port is chosen once and kept.
    if settings.ensure_torrent_port() {
        if let Err(e) = settings.save(&paths.settings) {
            tracing::warn!("saving the torrent port: {e}");
        }
    }
    tracing::info!(data_dir = %paths.data_dir.display(), "ODM starting");
    // The browser host and "Start ODM with Windows" start ODM with --background so no window
    // pops up; after "Update" it comes back with its window.
    let after_update = updates::take_show_marker(&paths.data_dir);
    let background = updates::starts_hidden(std::env::args(), after_update);
    let mut context = tauri::generate_context!();
    let endpoint = updates::endpoint_override(std::env::var(updates::ENDPOINT_ENV).ok().as_deref());
    let pubkey = updates::prepare_config(&mut context.config_mut().plugins.0, endpoint.as_ref());
    let updates_supported = updates::has_pubkey(&pubkey);
    if !updates_supported {
        tracing::info!("this build has no update key; update checks are off");
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if !argv.iter().any(|a| a == "--background") {
                shell::show_main(app);
            }
            // A magnet link or .torrent opened while ODM runs.
            if let Some(input) = torrent_arg::torrent_arg(&argv) {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = commands::open_torrent(&app, input).await {
                        tracing::warn!("opening a torrent from the command line: {e}");
                    }
                });
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        // "Start ODM with Windows": the Run entry starts ODM hidden in the tray.
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("ODM")
                .arg("--background")
                .build(),
        )
        // Registered in every build; without a public key ODM never calls it.
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            let contact = Arc::new(Contact::default());
            let power = Arc::new(Power::new(shell::ShellPower {
                system: SystemPower::new(),
                app: app.handle().clone(),
            }));
            app.manage(AppState {
                manager: manager.clone(),
                settings: settings.clone(),
                paths,
                offers: offers.clone(),
                pending: pending.clone(),
                ipc_stop: ipc_stop.clone(),
                completions: Mutex::new(CompletionTracker::default()),
                quitting: AtomicBool::new(false),
                contact: contact.clone(),
                pipe_running: AtomicBool::new(false),
                tools_busy: AtomicBool::new(false),
                power: power.clone(),
                settings_save: tokio::sync::Mutex::new(()),
            });
            let memory = app.state::<AppState>().paths.data_dir.join("updates.json");
            app.manage(updates::Updates::new(
                updates_supported,
                endpoint.map(|e| e.url),
                memory,
            ));
            if updates_supported {
                tauri::async_runtime::spawn(updates::run(app.handle().clone()));
            }
            // Keep the log flushing for as long as the app runs.
            app.manage(Mutex::new(log_guard));

            let handle = app.handle().clone();
            let power_handle = handle.clone();
            tauri::async_runtime::spawn(odm_appcore::power::run(
                manager.clone(),
                settings.clone(),
                power,
                move |ev: UiEvent| {
                    if let Err(e) = power_handle.emit(EVENT, &ev) {
                        tracing::warn!("could not send an event to the window: {e}");
                    }
                },
            ));
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
                contact,
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
                Ok(_) => app
                    .state::<AppState>()
                    .pipe_running
                    .store(true, Ordering::SeqCst),
                Err(e) => tracing::error!("browser integration is unavailable: {e}"),
            }
            let tools_dir = app.state::<AppState>().paths.tools_dir.clone();
            let now = app.state::<AppState>().settings.lock().unwrap().clone();
            tauri::async_runtime::spawn(async move {
                odm_appcore::tools::update_ytdlp_if_stale(&tools_dir, &now).await;
            });
            if let Ok(app_exe) = std::env::current_exe() {
                let host_exe = app_exe.with_file_name(odm_core::ids::host_exe_name());
                std::thread::spawn(move || registration::ensure_registered(&host_exe, &app_exe));
            }

            // ODM started by opening a magnet link or .torrent file.
            if let Some(input) = torrent_arg::torrent_arg(&std::env::args().collect::<Vec<_>>()) {
                let app = handle.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = commands::open_torrent(&app, input).await {
                        tracing::warn!("opening a torrent from the command line: {e}");
                    }
                });
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
            commands::add_torrent,
            commands::torrent_files,
            commands::set_torrent_files,
            commands::pick_torrent_file,
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
            commands::pick_program,
            commands::default_settings,
            commands::open_app_folder,
            commands::list_offers,
            commands::resolve_offer,
            commands::list_pending_confirms,
            commands::confirm_download,
            commands::refresh_url,
            commands::integration_status,
            commands::integration_test,
            commands::integration_repair,
            commands::open_extension_folder,
            commands::open_browser_extensions_page,
            commands::open_store_page,
            commands::store_links,
            commands::tools_status,
            commands::tools_install,
            commands::answer_exists,
            commands::set_download_limit,
            commands::cancel_after_queue,
            updates::update_status,
            updates::update_check,
            updates::update_dismiss,
            updates::update_install,
        ])
        .build(context)
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
