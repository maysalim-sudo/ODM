use odm_appcore::completion::{completion_action, CompletionAction};
use odm_appcore::dto::{DownloadDto, Finished, UiEvent};
use odm_appcore::power::{PowerActions, SystemPower};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, Window, WindowEvent, Wry};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use std::sync::atomic::Ordering;

use crate::speed_menu;
use crate::AppState;

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// The checkable items of the tray's "Speed limit" submenu: the presets, then Custom.
struct SpeedItems(Vec<(&'static str, CheckMenuItem<Wry>)>);

/// Checks the item for the limit now in force.
pub fn sync_speed_menu(app: &AppHandle, kbps: u64) {
    let Some(items) = app.try_state::<SpeedItems>() else {
        return;
    };
    let on = speed_menu::checked_id(kbps);
    for (id, item) in &items.0 {
        let _ = item.set_checked(*id == on);
    }
}

fn speed_submenu(app: &App, kbps: u64) -> tauri::Result<Submenu<Wry>> {
    let on = speed_menu::checked_id(kbps);
    let mut items = Vec::new();
    for (id, label, _) in speed_menu::PRESETS {
        items.push((
            id,
            CheckMenuItem::with_id(app, id, label, true, id == on, None::<&str>)?,
        ));
    }
    let custom = speed_menu::CUSTOM_ID;
    items.push((
        custom,
        CheckMenuItem::with_id(app, custom, "Custom…", true, custom == on, None::<&str>)?,
    ));
    let refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = items
        .iter()
        .map(|(_, i)| i as &dyn tauri::menu::IsMenuItem<Wry>)
        .collect();
    let sub = Submenu::with_items(app, "Speed limit", true, &refs)?;
    app.manage(SpeedItems(items));
    Ok(sub)
}

/// A preset from the tray: saves the limit the way the Settings page does.
fn set_speed_limit(app: &AppHandle, kbps: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let mut next = state.settings.lock().unwrap().clone();
        next.speed_limit_kbps = kbps;
        if let Err(e) = crate::commands::apply_settings(&app, &state, next).await {
            tracing::warn!("tray speed limit failed: {e}");
            sync_speed_menu(&app, state.settings.lock().unwrap().speed_limit_kbps);
        }
    });
}

pub fn setup(app: &mut App, background: bool) -> tauri::Result<()> {
    let kbps = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .speed_limit_kbps;
    let speed = speed_submenu(app, kbps)?;
    let show = MenuItem::with_id(app, "show", "Show ODM", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause_all", "Pause all", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume_all", "Resume all", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &pause,
            &resume,
            &PredefinedMenuItem::separator(app)?,
            &speed,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("ODM")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "pause_all" | "resume_all" => {
                let app = app.clone();
                let pause = event.id.as_ref() == "pause_all";
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    let r = if pause {
                        state.manager.pause_all().await
                    } else {
                        state.manager.resume_all().await
                    };
                    if let Err(e) = r {
                        tracing::warn!("tray action failed: {e}");
                    }
                });
            }
            "quit" => request_quit(app),
            speed_menu::CUSTOM_ID => {
                // Custom is typed in the Settings page, at its Speed section.
                show_main(app);
                let _ = app.emit(crate::OPEN_SETTINGS_EVENT, "speed");
                if let Some(state) = app.try_state::<AppState>() {
                    sync_speed_menu(app, state.settings.lock().unwrap().speed_limit_kbps);
                }
            }
            id => {
                if let Some(kbps) = speed_menu::preset_kbps(id) {
                    set_speed_limit(app, kbps);
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;

    let start_minimized = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .start_minimized;
    if !(start_minimized || background) {
        show_main(app.handle());
    }
    Ok(())
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        let app = window.app_handle();
        // No state means startup failed; let the window close normally.
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        // Either way the window does not close by itself: hide it, or run the quit flow.
        api.prevent_close();
        let to_tray = state.settings.lock().unwrap().close_to_tray;
        if to_tray {
            let _ = window.hide();
        } else {
            request_quit(app);
        }
    }
}

pub fn request_quit(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if state.quitting.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let manager = app.state::<AppState>().manager.clone();
        let active = manager.active_count().await.unwrap_or(0);
        if active > 0 {
            let text = if active == 1 {
                "1 download is still running. It will continue the next time you open ODM."
                    .to_string()
            } else {
                format!(
                    "{active} downloads are still running. They will continue the next time you open ODM."
                )
            };
            let dialog_app = app.clone();
            let confirmed = tauri::async_runtime::spawn_blocking(move || {
                dialog_app
                    .dialog()
                    .message(text)
                    .title("Quit ODM")
                    .kind(MessageDialogKind::Warning)
                    .buttons(MessageDialogButtons::OkCancelCustom(
                        "Quit".into(),
                        "Keep running".into(),
                    ))
                    .blocking_show()
            })
            .await
            .unwrap_or(false);
            if !confirmed {
                app.state::<AppState>()
                    .quitting
                    .store(false, Ordering::SeqCst);
                return;
            }
        }
        manager.shutdown().await;
        // "Install when I quit ODM": the installer runs now; this returns if there is none.
        crate::updates::install_on_quit(&app).await;
        app.exit(0);
    });
}

pub fn on_ui_event(app: &AppHandle, ev: &UiEvent) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if let UiEvent::ExistsPrompt { .. } = ev {
        // The download waits for an answer, even with ODM hidden in the tray: bring it forward.
        show_on_main_thread(app);
        return;
    }
    let Some((download, how)) = state.completions.lock().unwrap().newly_finished(ev) else {
        return;
    };
    let (wanted, title, on_complete) = {
        let s = state.settings.lock().unwrap();
        match how {
            Finished::Completed => (s.notify_complete, "Download complete", Some(s.on_complete)),
            Finished::Failed => (s.notify_failed, "Download failed", None),
        }
    };
    if let Some(action) = on_complete.and_then(|c| completion_action(c, &download.filename)) {
        open_finished(app, download, action);
    }
    if wanted {
        notify(app, title, &download.filename);
    }
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::debug!("notification failed: {e}");
    }
}

fn show_on_main_thread(app: &AppHandle) {
    let h = app.clone();
    if let Err(e) = app.run_on_main_thread(move || show_main(&h)) {
        tracing::warn!("could not show the window: {e}");
    }
}

/// "When a download finishes": show the file in its folder, or open it.
fn open_finished(app: &AppHandle, d: &DownloadDto, action: CompletionAction) {
    let file = std::path::Path::new(&d.save_dir).join(&d.filename);
    let r = match action {
        CompletionAction::OpenFolder => app.opener().reveal_item_in_dir(&file),
        CompletionAction::OpenFile => app.opener().open_path(file.to_string_lossy(), None::<&str>),
    };
    if let Err(e) = r {
        tracing::warn!("could not open the finished download: {e}");
    }
}

/// The side effects of "Prevent sleep" and "When the queue is done", plus quitting ODM.
pub struct ShellPower {
    pub system: SystemPower,
    pub app: AppHandle,
}

impl PowerActions for ShellPower {
    fn keep_awake(&self, on: bool) {
        self.system.keep_awake(on);
    }
    fn sleep(&self) {
        self.system.sleep();
    }
    fn shutdown(&self) -> Result<(), String> {
        let r = self.system.shutdown();
        let text = match &r {
            Ok(()) => "ODM will shut down the PC in 60 s.",
            Err(_) => "ODM couldn't shut down the PC.",
        };
        notify(&self.app, "The queue is done", text);
        show_on_main_thread(&self.app);
        r
    }
    fn cancel_shutdown(&self) {
        self.system.cancel_shutdown();
    }
    fn quit(&self) {
        request_quit(&self.app);
    }
}
