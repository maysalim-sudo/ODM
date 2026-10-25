//! What ODM does with the PC: keep it awake while downloading, and the "When the queue
//! is done" action (sleep, shut down, quit). Every real side effect goes through
//! `PowerActions`, so tests use a fake and never touch this machine.
use crate::dto::UiEvent;
use crate::settings::AfterQueue;
use crate::Settings;
use odm_engine::{EngineEvent, ManagerHandle};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;

/// Seconds between "the queue is done" and the after-queue action (sleep, shut down, quit).
pub const AFTER_QUEUE_DELAY_S: u32 = 60;
/// Seconds between "ODM will shut down the PC" and the shutdown.
pub const SHUTDOWN_DELAY_S: u32 = AFTER_QUEUE_DELAY_S;

/// Shown when Windows does not accept the shutdown.
pub const SHUTDOWN_FAILED: &str = "ODM couldn't shut down the PC.";

/// What the notification and the dialog say when the countdown starts.
pub fn pending_text(action: AfterQueue) -> &'static str {
    match action {
        AfterQueue::Sleep => "ODM will put the PC to sleep in 60 s.",
        AfterQueue::Shutdown => "ODM will shut down the PC in 60 s.",
        AfterQueue::Quit => "ODM will quit in 60 s.",
        AfterQueue::Nothing => "",
    }
}

/// The side effects. The real one is `SystemPower` (Windows and Linux, plus the app's quit).
pub trait PowerActions: Send + Sync + 'static {
    /// Keep the PC from sleeping (on) or let it sleep again (off).
    fn keep_awake(&self, on: bool);
    /// Put the PC to sleep now.
    fn sleep(&self);
    /// Shut the PC down in `SHUTDOWN_DELAY_S` seconds (`shutdown /s /t 60`). An error
    /// means Windows did not accept it, so no shutdown is coming.
    fn shutdown(&self) -> Result<(), String>;
    /// Cancel a shutdown asked for by `shutdown` (`shutdown /a`).
    fn cancel_shutdown(&self);
    /// Quit ODM.
    fn quit(&self);
    /// The countdown to sleep or quit started: tell the user (a notification). `shutdown`
    /// tells the user itself.
    fn announce(&self, _action: AfterQueue) {}
}

#[derive(Default)]
struct PowerState {
    awake: bool,
    /// The after-queue action counting down, and its countdown's id.
    pending: Option<(AfterQueue, u64)>,
    last_id: u64,
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

    fn lock(&self) -> std::sync::MutexGuard<'_, PowerState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Keeps the PC awake while `active` downloads run and `prevent_sleep` is on.
    pub fn set_activity(&self, active: usize, prevent_sleep: bool) {
        let want = prevent_sleep && active > 0;
        let mut st = self.lock();
        if st.awake != want {
            st.awake = want;
            self.actions.keep_awake(want);
        }
    }

    /// The after-queue action counting down and its id, if any.
    pub fn pending(&self) -> Option<(AfterQueue, u64)> {
        self.lock().pending
    }

    /// The queue has nothing left: start the chosen action's 60 s countdown. Returns what
    /// the UI should show. Sleep and quit happen when `fire` is called with the countdown's
    /// id; Windows itself counts down a shutdown (`shutdown /t 60`).
    pub fn queue_done(&self, after: AfterQueue) -> Option<UiEvent> {
        if after == AfterQueue::Nothing {
            return None;
        }
        let mut st = self.lock();
        if let Some((action, _)) = st.pending {
            // A second "done" while one counts down does not start another.
            return Some(UiEvent::AfterQueuePending {
                action,
                seconds: AFTER_QUEUE_DELAY_S,
            });
        }
        if after == AfterQueue::Shutdown {
            tracing::info!("the queue is done; shutting down in {SHUTDOWN_DELAY_S} s");
            if let Err(e) = self.actions.shutdown() {
                tracing::warn!("could not shut down the PC: {e}");
                return Some(UiEvent::ShutdownFailed {
                    message: SHUTDOWN_FAILED.into(),
                });
            }
        } else {
            tracing::info!("the queue is done; {after:?} in {AFTER_QUEUE_DELAY_S} s");
            self.actions.announce(after);
        }
        st.last_id += 1;
        st.pending = Some((after, st.last_id));
        Some(UiEvent::AfterQueuePending {
            action: after,
            seconds: AFTER_QUEUE_DELAY_S,
        })
    }

    /// The countdown `id` ran out: put the PC to sleep or quit, unless it was cancelled
    /// meanwhile. Sleep returns only after the PC wakes, so call this on a blocking thread.
    /// Returns what the UI should show.
    pub fn fire(&self, id: u64) -> Option<UiEvent> {
        let action = {
            let mut st = self.lock();
            match st.pending {
                Some((a @ (AfterQueue::Sleep | AfterQueue::Quit), n)) if n == id => {
                    st.pending = None;
                    a
                }
                _ => return None,
            }
        };
        if action == AfterQueue::Sleep {
            tracing::info!("putting the PC to sleep");
            self.actions.sleep();
        } else {
            tracing::info!("quitting");
            self.actions.quit();
        }
        Some(UiEvent::AfterQueueDone)
    }

    /// Cancels the countdown (and a pending shutdown, with `shutdown /a`). Returns what the
    /// UI should show, or None if nothing was counting down.
    pub fn cancel_pending(&self) -> Option<UiEvent> {
        let (action, _) = self.lock().pending.take()?;
        tracing::info!("the after-queue action ({action:?}) was cancelled");
        if action == AfterQueue::Shutdown {
            self.actions.cancel_shutdown();
        }
        Some(UiEvent::AfterQueueCancelled)
    }
}

/// Waits `delay`, then runs the countdown `id`'s action unless it was cancelled meanwhile.
pub async fn countdown<P: PowerActions>(
    power: Arc<Power<P>>,
    id: u64,
    delay: Duration,
) -> Option<UiEvent> {
    tokio::time::sleep(delay).await;
    match tokio::task::spawn_blocking(move || power.fire(id)).await {
        Ok(ev) => ev,
        Err(e) => {
            tracing::warn!("the after-queue action stopped: {e}");
            None
        }
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
    let delay = Duration::from_secs(AFTER_QUEUE_DELAY_S.into());
    run_with_delay(manager, settings, power, emit, delay).await
}

/// `run` with the countdown before sleep and quit set to `delay` (tests use a short one).
pub async fn run_with_delay<P, F>(
    manager: ManagerHandle,
    settings: Arc<Mutex<Settings>>,
    power: Arc<Power<P>>,
    emit: F,
    delay: Duration,
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
    // Results of the after-queue work that runs off this loop, for the UI.
    let (done_tx, mut done_rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();
    // A download runs: the queue is no longer done. `shutdown /a` runs a program, so the
    // cancel goes to a blocking thread.
    let cancel_if_busy = |active: usize| {
        if active > 0 && power.pending().is_some() {
            let p = power.clone();
            let tx = done_tx.clone();
            tokio::spawn(async move {
                if let Ok(Some(ev)) = tokio::task::spawn_blocking(move || p.cancel_pending()).await
                {
                    let _ = tx.send(ev);
                }
            });
        }
    };
    loop {
        let event = tokio::select! {
            ev = rx.recv() => ev,
            Some(ui) = done_rx.recv() => {
                let started = matches!(ui, UiEvent::AfterQueuePending { .. });
                emit(ui);
                if started {
                    // A download may have started while the countdown was being set up.
                    cancel_if_busy(manager.active_count().await.unwrap_or(0));
                }
                continue;
            }
        };
        match event {
            Ok(EngineEvent::QueueDone) => {
                let after = settings
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .after_queue;
                // Shutdown runs a program and sleep returns only after the PC wakes: they
                // run on a blocking thread, and this loop keeps following events.
                let p = power.clone();
                let tx = done_tx.clone();
                tokio::spawn(async move {
                    let q = p.clone();
                    match tokio::task::spawn_blocking(move || q.queue_done(after)).await {
                        Ok(Some(ev)) => {
                            let _ = tx.send(ev);
                        }
                        Ok(None) => return,
                        Err(e) => {
                            tracing::warn!("the after-queue action stopped: {e}");
                            return;
                        }
                    }
                    if let Some((AfterQueue::Sleep | AfterQueue::Quit, id)) = p.pending() {
                        if let Some(ev) = countdown(p, id, delay).await {
                            let _ = tx.send(ev);
                        }
                    }
                });
            }
            Ok(
                EngineEvent::State { .. } | EngineEvent::Added { .. } | EngineEvent::Removed { .. },
            )
            | Err(RecvError::Lagged(_)) => {
                let active = manager.active_count().await.unwrap_or(0);
                power.set_activity(active, prevent());
                cancel_if_busy(active);
            }
            Ok(_) => {}
            Err(RecvError::Closed) => break,
        }
    }
    power.set_activity(0, false);
}

/// The real side effects on Windows and Linux. `quit` does nothing here: the app quits itself.
pub struct SystemPower {
    #[cfg(windows)]
    awake: std::sync::mpsc::Sender<bool>,
    /// Linux: the `systemd-inhibit` child holding a sleep/idle lock while it runs.
    #[cfg(unix)]
    inhibitor: Mutex<Option<std::process::Child>>,
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
        #[cfg(unix)]
        Self {
            inhibitor: Mutex::new(None),
        }
    }
}

/// Linux: blocks sleep and idle while the child runs. The child (`tail --pid`) also ends when
/// ODM does, so a crash never leaves the lock behind. None if systemd-inhibit is missing.
#[cfg(unix)]
fn start_inhibitor() -> Option<std::process::Child> {
    use std::process::{Command, Stdio};
    let child = Command::new("systemd-inhibit")
        .args([
            "--what=sleep:idle",
            "--who=ODM",
            "--why=Downloading",
            "--mode=block",
            "tail",
            &format!("--pid={}", std::process::id()),
            "-f",
            "/dev/null",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match child {
        Ok(c) => Some(c),
        Err(e) => {
            tracing::warn!("could not keep the PC awake (systemd-inhibit): {e}");
            None
        }
    }
}

#[cfg(unix)]
impl Drop for SystemPower {
    fn drop(&mut self) {
        self.keep_awake(false);
    }
}

impl PowerActions for SystemPower {
    fn keep_awake(&self, on: bool) {
        #[cfg(windows)]
        let _ = self.awake.send(on);
        #[cfg(unix)]
        {
            let mut held = self.inhibitor.lock().unwrap_or_else(|e| e.into_inner());
            // A child that exited on its own (systemd-inhibit refused) no longer holds a lock.
            if held
                .as_mut()
                .is_some_and(|c| !matches!(c.try_wait(), Ok(None)))
            {
                *held = None;
            }
            match (on, held.take()) {
                (true, None) => *held = start_inhibitor(),
                (true, Some(c)) => *held = Some(c),
                (false, Some(mut c)) => {
                    let _ = c.kill();
                    let _ = c.wait();
                }
                (false, None) => {}
            }
        }
    }

    fn sleep(&self) {
        #[cfg(windows)]
        win::sleep();
        #[cfg(unix)]
        if let Err(e) = run_quiet("systemctl", &["suspend"]) {
            tracing::warn!("{e}");
        }
    }

    fn shutdown(&self) -> Result<(), String> {
        #[cfg(windows)]
        let args = ["/s".to_string(), "/t".into(), SHUTDOWN_DELAY_S.to_string()];
        #[cfg(unix)]
        let args = [
            "-P".to_string(),
            format!("+{}", SHUTDOWN_DELAY_S.div_ceil(60)),
        ];
        run_shutdown(&args.iter().map(String::as_str).collect::<Vec<_>>())
    }

    fn cancel_shutdown(&self) {
        #[cfg(windows)]
        let args = ["/a"];
        #[cfg(unix)]
        let args = ["-c"];
        if let Err(e) = run_shutdown(&args) {
            tracing::warn!("{e}");
        }
    }

    fn quit(&self) {}
}

/// Runs the system's `shutdown` tool (without a console window on Windows).
fn run_shutdown(args: &[&str]) -> Result<(), String> {
    run_quiet("shutdown", args)
}

fn run_quiet(program: &str, args: &[&str]) -> Result<(), String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    #[cfg(unix)]
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("{program} {args:?} exited with {s}")),
        Err(e) => Err(format!("could not run {program} {args:?}: {e}")),
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
    pub struct FakePower(
        pub Mutex<Vec<&'static str>>,
        pub std::sync::atomic::AtomicBool,
    );

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
        fn shutdown(&self) -> Result<(), String> {
            self.0.lock().unwrap().push("shutdown");
            if self.1.load(std::sync::atomic::Ordering::SeqCst) {
                Err("shutdown.exe exited with 1190".into())
            } else {
                Ok(())
            }
        }
        fn cancel_shutdown(&self) {
            self.0.lock().unwrap().push("cancel_shutdown");
        }
        fn quit(&self) {
            self.0.lock().unwrap().push("quit");
        }
        fn announce(&self, _action: AfterQueue) {
            self.0.lock().unwrap().push("announce");
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

    fn pending(action: AfterQueue) -> Option<UiEvent> {
        Some(UiEvent::AfterQueuePending {
            action,
            seconds: AFTER_QUEUE_DELAY_S,
        })
    }

    #[test]
    fn the_countdown_copy_names_the_action() {
        assert_eq!(
            pending_text(AfterQueue::Sleep),
            "ODM will put the PC to sleep in 60 s."
        );
        assert_eq!(
            pending_text(AfterQueue::Shutdown),
            "ODM will shut down the PC in 60 s."
        );
        assert_eq!(pending_text(AfterQueue::Quit), "ODM will quit in 60 s.");
    }

    #[test]
    fn sleep_and_quit_wait_for_the_countdown() {
        for (after, call) in [(AfterQueue::Sleep, "sleep"), (AfterQueue::Quit, "quit")] {
            let p = Power::new(FakePower::default());
            assert_eq!(p.queue_done(AfterQueue::Nothing), None);
            assert_eq!(p.queue_done(after), pending(after));
            assert_eq!(p.actions().calls(), ["announce"], "nothing runs at once");
            let (_, id) = p.pending().unwrap();
            // A second "done" while counting down does not start another countdown.
            assert_eq!(p.queue_done(after), pending(after));
            assert_eq!(p.pending().unwrap().1, id);
            assert_eq!(p.fire(id + 1), None, "a stale countdown does nothing");
            assert_eq!(p.fire(id), Some(UiEvent::AfterQueueDone));
            assert_eq!(p.actions().calls(), ["announce", call]);
            assert_eq!(p.fire(id), None, "it runs once");
            assert_eq!(p.pending(), None);
        }
    }

    #[test]
    fn cancelling_sleep_or_quit_only_stops_the_timer() {
        for after in [AfterQueue::Sleep, AfterQueue::Quit] {
            let p = Power::new(FakePower::default());
            assert_eq!(p.cancel_pending(), None, "nothing to cancel");
            p.queue_done(after);
            let (_, id) = p.pending().unwrap();
            assert_eq!(p.cancel_pending(), Some(UiEvent::AfterQueueCancelled));
            assert_eq!(p.fire(id), None, "the countdown ran out after the cancel");
            assert_eq!(p.actions().calls(), ["announce"], "no shutdown /a either");
            assert_eq!(p.cancel_pending(), None);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn sleep_runs_60_s_after_the_queue_is_done() {
        let p = Arc::new(Power::new(FakePower::default()));
        p.queue_done(AfterQueue::Sleep);
        let (_, id) = p.pending().unwrap();
        let timer = tokio::spawn(countdown(p.clone(), id, Duration::from_secs(60)));
        tokio::time::advance(Duration::from_secs(59)).await;
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert_eq!(p.actions().calls(), ["announce"], "not yet at 59 s");
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(timer.await.unwrap(), Some(UiEvent::AfterQueueDone));
        assert_eq!(p.actions().calls(), ["announce", "sleep"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancelled_quit_never_runs() {
        let p = Arc::new(Power::new(FakePower::default()));
        p.queue_done(AfterQueue::Quit);
        let (_, id) = p.pending().unwrap();
        let timer = tokio::spawn(countdown(p.clone(), id, Duration::from_secs(60)));
        tokio::time::advance(Duration::from_secs(30)).await;
        assert_eq!(p.cancel_pending(), Some(UiEvent::AfterQueueCancelled));
        tokio::time::advance(Duration::from_secs(30)).await;
        assert_eq!(timer.await.unwrap(), None);
        assert_eq!(p.actions().calls(), ["announce"]);
    }

    #[test]
    fn a_failed_shutdown_is_reported_and_not_counted_down() {
        let p = Power::new(FakePower::default());
        p.actions()
            .1
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            p.queue_done(AfterQueue::Shutdown),
            Some(UiEvent::ShutdownFailed {
                message: "ODM couldn't shut down the PC.".into()
            })
        );
        // Nothing is pending, so there is nothing to cancel, and a later try asks again.
        assert_eq!(p.cancel_pending(), None);
        p.actions()
            .1
            .store(false, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            p.queue_done(AfterQueue::Shutdown),
            pending(AfterQueue::Shutdown)
        );
        assert_eq!(p.actions().calls(), ["shutdown", "shutdown"]);
    }

    #[test]
    fn shutdown_waits_a_minute_and_can_be_cancelled() {
        let p = Power::new(FakePower::default());
        assert_eq!(p.cancel_pending(), None, "nothing to cancel");
        assert!(p.actions().calls().is_empty());
        assert_eq!(
            p.queue_done(AfterQueue::Shutdown),
            pending(AfterQueue::Shutdown)
        );
        assert_eq!(p.actions().calls(), ["shutdown"]);
        // A second "done" while one is pending does not ask Windows twice.
        assert_eq!(
            p.queue_done(AfterQueue::Shutdown),
            pending(AfterQueue::Shutdown)
        );
        assert_eq!(p.actions().calls(), ["shutdown"]);
        // Windows counts the shutdown down; ODM's timer never fires it.
        let (_, id) = p.pending().unwrap();
        assert_eq!(p.fire(id), None);
        assert_eq!(p.cancel_pending(), Some(UiEvent::AfterQueueCancelled));
        assert_eq!(p.actions().calls(), ["shutdown", "cancel_shutdown"]);
        assert_eq!(p.cancel_pending(), None);
    }
}
