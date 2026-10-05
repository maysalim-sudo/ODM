#![cfg(windows)]

use odm_appcore::confirm::Pending;
use odm_appcore::integration::{self_test, Contact};
use odm_appcore::ipc::IpcContext;
use odm_appcore::offers::Offers;
use odm_appcore::pipe::serve;
use odm_appcore::summary::SpeedBoard;
use odm_appcore::Settings;
use odm_engine::{EngineConfig, ManagerHandle};
use odm_store::Store;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// `target/<profile>/odm-host.exe`, next to the `deps` folder this test runs from.
fn built_host() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.parent()?.join("odm-host.exe");
    p.exists().then_some(p)
}

#[tokio::test(flavor = "multi_thread")]
async fn self_test_goes_through_the_real_host_and_pipe() {
    let Some(host) = built_host() else {
        eprintln!("skipped: run `cargo build -p odm-host` first");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let cfg = EngineConfig {
        download_dir: dir.path().to_path_buf(),
        ..Default::default()
    };
    let manager = ManagerHandle::start(cfg, Store::open_in_memory().unwrap())
        .await
        .unwrap();
    let contact = Arc::new(Contact::default());
    let ctx = Arc::new(IpcContext {
        manager,
        settings: Arc::new(Mutex::new(Settings::default())),
        speeds: Arc::new(Mutex::new(SpeedBoard::default())),
        offers: Arc::new(Offers::default()),
        show_window: Box::new(|| {}),
        on_offer: Box::new(|_| {}),
        pending: Arc::new(Pending::default()),
        on_confirm: Box::new(|_| {}),
        contact: contact.clone(),
    });
    let name = format!(r"\\.\pipe\odm-test-{}", uuid::Uuid::new_v4());
    let stop = CancellationToken::new();
    serve(name.clone(), ctx, stop.clone()).await.unwrap();
    // No other test in this binary spawns children, so setting the variable here is safe.
    std::env::set_var("ODM_PIPE_NAME", &name);
    let result = tokio::task::spawn_blocking(move || self_test(&host))
        .await
        .unwrap();
    stop.cancel();
    assert_eq!(result, Ok(env!("CARGO_PKG_VERSION").to_string()));
    assert_eq!(contact.get().unwrap().1, "PING");
}
