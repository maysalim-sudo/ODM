use odm_appcore::completion::{completion_action, CompletionAction};
use odm_appcore::dto::{DownloadDto, Finished, UiEvent};
use odm_appcore::power::{PowerActions, SystemPower};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, Window, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use std::sync::atomic::Ordering;

use crate::AppState;

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn setup(app: &mut App, background: bool) -> tauri::Result<()> {
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
            _ => {}
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
    fn shutdown(&self) {
        self.system.shutdown();
        notify(
            &self.app,
            "The queue is done",
            "ODM will shut down the PC in 60 s.",
        );
        show_on_main_thread(&self.app);
    }
    fn cancel_shutdown(&self) {
        self.system.cancel_shutdown();
    }
    fn quit(&self) {
        request_quit(&self.app);
    }
}
