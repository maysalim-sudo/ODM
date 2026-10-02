use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                w.show()?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ODM");
}
