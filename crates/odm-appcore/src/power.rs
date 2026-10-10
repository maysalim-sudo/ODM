//! What ODM does with the PC: keep it awake while downloading, and the "When the queue
//! is done" action (sleep, shut down, quit). Every real side effect goes through
//! `PowerActions`, so tests use a fake and never touch this machine.
use crate::dto::UiEvent;
use crate::settings::AfterQueue;
use crate::Settings;
use odm_engine::{EngineEvent, ManagerHandle};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast::error::RecvError;

/// Seconds between "ODM will shut down the PC" and the shutdown.
pub const SHUTDOWN_DELAY_S: u32 = 60;

/// The side effects. The real one is `SystemPower` (Windows only, plus the app's quit).
pub trait PowerActions: Send + Sync + 'static {
    /// Keep the PC from sleeping (on) or let it sleep again (off).
    fn keep_awake(&self, on: bool);
    /// Put the PC to sleep now.
    fn sleep(&self);
    /// Shut the PC down in `SHUTDOWN_DELAY_S` seconds (`shutdown /s /t 60`).
    fn shutdown(&self);
    /// Cancel a shutdown asked for by `shutdown` (`shutdown /a`).
    fn cancel_shutdown(&self);
    /// Quit ODM.
    fn quit(&self);
}

#[derive(Default)]
struct PowerState {
    awake: bool,
    shutdown_pending: bool,
}

pub struct Power<P: PowerActions> {
    actions: P,
    state: Mutex<PowerState>,
}

impl<P: PowerActions> Power<P> {
    pub fn new(actions: P) -> Self {
        Self {
            actions,
            state: Mutex::default(),
        }
    }

    pub fn actions(&self) -> &P {
        &self.actions
    }

    /// Keeps the PC awake while `active` downloads run and `prevent_sleep` is on.
    pub fn set_activity(&self, active: usize, prevent_sleep: bool) {
        let want = prevent_sleep && active > 0;
        let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if st.awake != want {
            st.awake = want;
            self.actions.keep_awake(want);
        }
    }

    /// The queue has nothing left: run the chosen action. Returns what the UI should show.
    pub fn queue_done(&self, after: AfterQueue) -> Option<UiEvent> {
        match after {
            AfterQueue::Nothing => None,
            AfterQueue::Sleep => {
                tracing::info!("the queue is done; putting the PC to sleep");
                self.actions.sleep();
                None
            }
            AfterQueue::Quit => {
                tracing::info!("the queue is done; quitting");
                self.actions.quit();
                None
            }
            AfterQueue::Shutdown => {
                let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
                if !st.shutdown_pending {
                    tracing::info!("the queue is done; shutting down in {SHUTDOWN_DELAY_S} s");
                    st.shutdown_pending = true;
                    self.actions.shutdown();
                }
                Some(UiEvent::ShutdownPending {
                    seconds: SHUTDOWN_DELAY_S,
                })
            }
        }
    }

    /// Cancels a pending shutdown. Returns what the UI should show, or None if none was pending.
    pub fn cancel_shutdown(&self) -> Option<UiEvent> {
        let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if !st.shutdown_pending {
            return None;
        }
        st.shutdown_pending = false;
        tracing::info!("shutdown cancelled");
        self.actions.cancel_shutdown();
        Some(UiEvent::ShutdownCancelled)
    }
}

/// Follows the engine's events for as long as it runs: activity for `prevent_sleep`, and
/// `QueueDone` for `after_queue`. `emit` gets the events the UI should show.
pub async fn run<P, F>(
    manager: ManagerHandle,
    settings: Arc<Mutex<Settings>>,
    power: Arc<Power<P>>,
    emit: F,
) where
    P: PowerActions,
    F: Fn(UiEvent),
{
    let mut rx = manager.subscribe();
    let prevent = || {
        settings
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .prevent_sleep
    };
    let active = manager.active_count().await.unwrap_or(0);
    power.set_activity(active, prevent());
    loop {
        match rx.recv().await {
            Ok(EngineEvent::QueueDone) => {
                let after = settings
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .after_queue;
                if let Some(ev) = power.queue_done(after) {
                    emit(ev);
                }
            }
            Ok(
                EngineEvent::State { .. } | EngineEvent::Added { .. } | EngineEvent::Removed { .. },
            )
            | Err(RecvError::Lagged(_)) => {
                let active = manager.active_count().await.unwrap_or(0);
                power.set_activity(active, prevent());
            }
            Ok(_) => {}
            Err(RecvError::Closed) => break,
        }
    }
    power.set_activity(0, false);
}

/// The real side effects on Windows. `quit` does nothing here: the app quits itself.
pub struct SystemPower {
    #[cfg(windows)]
    awake: std::sync::mpsc::Sender<bool>,
}

impl Default for SystemPower {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemPower {
    pub fn new() -> Self {
        #[cfg(windows)]
        {
            // SetThreadExecutionState belongs to the calling thread, so one thread owns it.
            let (tx, rx) = std::sync::mpsc::channel::<bool>();
            let spawned = std::thread::Builder::new()
                .name("odm-awake".into())
                .spawn(move || {
                    for on in rx {
                        win::set_awake(on);
                    }
                    win::set_awake(false);
                });
            if let Err(e) = spawned {
                tracing::warn!("could not start the keep-awake thread: {e}");
            }
            Self { awake: tx }
        }
        #[cfg(not(windows))]
        Self {}
    }
}

impl PowerActions for SystemPower {
    fn keep_awake(&self, on: bool) {
        #[cfg(windows)]
        let _ = self.awake.send(on);
        #[cfg(not(windows))]
        let _ = on;
    }

    fn sleep(&self) {
        #[cfg(windows)]
        win::sleep();
    }

    fn shutdown(&self) {
        run_shutdown(&["/s", "/t", &SHUTDOWN_DELAY_S.to_string()]);
    }

    fn cancel_shutdown(&self) {
        run_shutdown(&["/a"]);
    }

    fn quit(&self) {}
}

/// Runs Windows' `shutdown` tool without a console window.
fn run_shutdown(args: &[&str]) {
    let mut cmd = std::process::Command::new("shutdown");
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    match cmd.status() {
        Ok(s) if s.success() => {}
        Ok(s) => tracing::warn!("shutdown {args:?} exited with {s}"),
        Err(e) => tracing::warn!("could not run shutdown {args:?}: {e}"),
    }
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LUID};
    use windows_sys::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
        TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Power::{
        SetSuspendState, SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    pub fn set_awake(on: bool) {
        let flags = if on {
            ES_CONTINUOUS | ES_SYSTEM_REQUIRED
        } else {
            ES_CONTINUOUS
        };
        // SAFETY: plain flags; no pointers.
        if unsafe { SetThreadExecutionState(flags) } == 0 {
            tracing::warn!("SetThreadExecutionState failed");
        }
    }

    /// Sleep needs the shutdown privilege, which a user has but is off by default.
    fn enable_shutdown_privilege() {
        let name: Vec<u16> = "SeShutdownPrivilege"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: valid pointers for the duration of each call; the token is closed below.
        unsafe {
            if OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                &mut token,
            ) == 0
            {
                tracing::warn!(
                    "OpenProcessToken failed: {}",
                    std::io::Error::last_os_error()
                );
                return;
            }
            let mut luid = LUID {
                LowPart: 0,
                HighPart: 0,
            };
            if LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) != 0 {
                let tp = TOKEN_PRIVILEGES {
                    PrivilegeCount: 1,
                    Privileges: [LUID_AND_ATTRIBUTES {
                        Luid: luid,
                        Attributes: SE_PRIVILEGE_ENABLED,
                    }],
                };
                AdjustTokenPrivileges(token, 0, &tp, 0, std::ptr::null_mut(), std::ptr::null_mut());
            }
            CloseHandle(token);
        }
    }

    pub fn sleep() {
        enable_shutdown_privilege();
        // SAFETY: plain flags: sleep (not hibernate), no forced suspend, wake events allowed.
        if !unsafe { SetSuspendState(false, false, false) } {
            tracing::warn!(
                "SetSuspendState failed: {}",
                std::io::Error::last_os_error()
            );
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Records the calls instead of making them.
    #[derive(Default)]
    pub struct FakePower(pub Mutex<Vec<&'static str>>);

    impl FakePower {
        pub fn calls(&self) -> Vec<&'static str> {
            self.0.lock().unwrap().clone()
        }
    }

    impl PowerActions for FakePower {
        fn keep_awake(&self, on: bool) {
            self.0
                .lock()
                .unwrap()
                .push(if on { "awake" } else { "may_sleep" });
        }
        fn sleep(&self) {
            self.0.lock().unwrap().push("sleep");
        }
        fn shutdown(&self) {
            self.0.lock().unwrap().push("shutdown");
        }
        fn cancel_shutdown(&self) {
            self.0.lock().unwrap().push("cancel_shutdown");
        }
        fn quit(&self) {
            self.0.lock().unwrap().push("quit");
        }
    }

    #[test]
    fn keeps_awake_only_while_downloading_and_only_when_on() {
        let p = Power::new(FakePower::default());
        p.set_activity(0, true);
        assert!(p.actions().calls().is_empty(), "idle: nothing to do");
        p.set_activity(2, true);
        p.set_activity(1, true);
        assert_eq!(p.actions().calls(), ["awake"], "set once, not per change");
        p.set_activity(0, true);
        assert_eq!(p.actions().calls(), ["awake", "may_sleep"]);
        p.set_activity(3, false);
        assert_eq!(
            p.actions().calls(),
            ["awake", "may_sleep"],
            "the setting is off"
        );
        p.set_activity(3, true);
        p.set_activity(3, false);
        assert_eq!(
            p.actions().calls(),
            ["awake", "may_sleep", "awake", "may_sleep"],
            "turning the setting off clears it at once"
        );
    }

    #[test]
    fn after_queue_actions() {
        let p = Power::new(FakePower::default());
        assert_eq!(p.queue_done(AfterQueue::Nothing), None);
        assert!(p.actions().calls().is_empty());
        assert_eq!(p.queue_done(AfterQueue::Sleep), None);
        assert_eq!(p.actions().calls(), ["sleep"]);
        assert_eq!(p.queue_done(AfterQueue::Quit), None);
        assert_eq!(p.actions().calls(), ["sleep", "quit"]);
    }

    #[test]
    fn shutdown_waits_a_minute_and_can_be_cancelled() {
        let p = Power::new(FakePower::default());
        assert_eq!(p.cancel_shutdown(), None, "nothing to cancel");
        assert!(p.actions().calls().is_empty());
        assert_eq!(
            p.queue_done(AfterQueue::Shutdown),
            Some(UiEvent::ShutdownPending {
                seconds: SHUTDOWN_DELAY_S
            })
        );
        assert_eq!(p.actions().calls(), ["shutdown"]);
        // A second "done" while one is pending does not ask Windows twice.
        assert_eq!(
            p.queue_done(AfterQueue::Shutdown),
            Some(UiEvent::ShutdownPending {
                seconds: SHUTDOWN_DELAY_S
            })
        );
        assert_eq!(p.actions().calls(), ["shutdown"]);
        assert_eq!(p.cancel_shutdown(), Some(UiEvent::ShutdownCancelled));
        assert_eq!(p.actions().calls(), ["shutdown", "cancel_shutdown"]);
        assert_eq!(p.cancel_shutdown(), None);
    }
}
