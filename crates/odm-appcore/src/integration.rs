//! Browser integration status: can each browser reach ODM, does the whole path work, and repair.
use crate::ipc::now_ms;
use odm_core::ipc::{cmd, read_frame, write_frame, Request, Response};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    Ok,
    Missing,
    WrongPath,
    /// The manifest file is unreadable or not valid JSON.
    Broken,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BrowserCheck {
    pub browser: String,
    pub status: Check,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IntegrationStatus {
    /// odm-host.exe exists next to the app exe.
    pub host_found: bool,
    pub pipe_running: bool,
    /// In this order: Chrome, Edge, Brave, Vivaldi, Opera, Chromium, Firefox and Floorp.
    pub browsers: Vec<BrowserCheck>,
    /// Epoch ms of the last IPC request from a browser.
    pub last_contact_ms: Option<u64>,
    pub last_command: Option<String>,
}

/// Registry keys (under HKCU) whose `\com.odm.host` default value is the manifest path.
/// Copied from odm-host's `register.rs` (`CHROMIUM_KEYS` and `MOZILLA_KEY`); appcore does not depend on odm-host.
pub const BROWSER_KEYS: [(&str, &str); 7] = [
    ("Chrome", r"Software\Google\Chrome\NativeMessagingHosts"),
    ("Edge", r"Software\Microsoft\Edge\NativeMessagingHosts"),
    (
        "Brave",
        r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts",
    ),
    ("Vivaldi", r"Software\Vivaldi\NativeMessagingHosts"),
    ("Opera", r"Software\Opera Software\NativeMessagingHosts"),
    ("Chromium", r"Software\Chromium\NativeMessagingHosts"),
    (
        "Firefox and Floorp",
        r"Software\Mozilla\NativeMessagingHosts",
    ),
];

const MISSING_HOST: &str = "The browser helper (odm-host.exe) is missing next to ODM.";

fn canon(s: &str) -> String {
    s.replace('/', "\\").to_lowercase()
}

/// Pure: given the registered manifest path and a way to read files, judge one browser's registration.
pub fn check_browser(
    browser: &str,
    manifest_path: Option<String>,
    read_file: impl Fn(&str) -> Option<String>,
    host_exe: &Path,
) -> BrowserCheck {
    let make = |status, detail: &str| BrowserCheck {
        browser: browser.to_string(),
        status,
        detail: detail.to_string(),
    };
    let Some(manifest) = manifest_path else {
        return make(Check::Missing, "Not registered.");
    };
    let broken = || make(Check::Broken, "The registration file is damaged.");
    let Some(text) = read_file(&manifest) else {
        return broken();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return broken();
    };
    let same = json
        .get("path")
        .and_then(|p| p.as_str())
        .is_some_and(|p| canon(p) == canon(&host_exe.to_string_lossy()));
    if same {
        make(Check::Ok, "Ready.")
    } else {
        make(Check::WrongPath, "Registered for another copy of ODM.")
    }
}

/// The last request a browser sent, shared with `ipc::dispatch`.
#[derive(Default)]
pub struct Contact {
    last: Mutex<Option<(u64, String)>>,
}

impl Contact {
    pub fn record(&self, cmd: &str) {
        let shown: String = cmd.chars().take(64).collect();
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Some((now_ms(), shown));
    }

    pub fn get(&self) -> Option<(u64, String)> {
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

#[cfg(windows)]
pub fn status(host_exe: &Path, pipe_running: bool, contact: &Contact) -> IntegrationStatus {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let browsers = BROWSER_KEYS
        .iter()
        .map(|(name, key)| {
            let manifest = hkcu
                .open_subkey(format!(r"{key}\{}", odm_core::ids::HOST_NAME))
                .ok()
                .and_then(|k| k.get_value::<String, _>("").ok())
                .filter(|v| !v.is_empty());
            check_browser(
                name,
                manifest,
                |p| std::fs::read_to_string(p).ok(),
                host_exe,
            )
        })
        .collect();
    let last = contact.get();
    IntegrationStatus {
        host_found: host_exe.exists(),
        pipe_running,
        browsers,
        last_contact_ms: last.as_ref().map(|(t, _)| *t),
        last_command: last.map(|(_, c)| c),
    }
}

/// Spawns `exe` without a console window where that applies.
#[cfg(windows)]
fn command(exe: &Path) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut c = std::process::Command::new(exe);
    c.creation_flags(0x0800_0000);
    c
}

#[cfg(not(windows))]
fn command(exe: &Path) -> std::process::Command {
    std::process::Command::new(exe)
}

/// Runs `<host_exe>` with ODM_PIPE_NAME inherited, writes one framed PING, reads one framed reply (timeout 10 s).
/// Ok(version) on ok:true; Err(plain sentence) otherwise.
pub fn self_test(host_exe: &Path) -> Result<String, String> {
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::Duration;

    if !host_exe.exists() {
        return Err(MISSING_HOST.into());
    }
    let mut child = command(host_exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "The browser helper could not be started.".to_string())?;
    let (Some(mut stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        return Err("The browser helper could not be started.".into());
    };
    let ping = Request {
        id: Some("self-test".into()),
        cmd: cmd::PING.into(),
        payload: serde_json::json!({}),
    };
    let body = serde_json::to_vec(&ping).unwrap_or_default();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let reply = write_frame(&mut stdin, &body)
            .ok()
            .and_then(|_| read_frame(&mut stdout).ok());
        let _ = tx.send(reply);
    });
    let reply = rx.recv_timeout(Duration::from_secs(10));
    let _ = child.kill();
    let _ = child.wait();
    let Some(bytes) = reply.ok().flatten() else {
        return Err("The browser helper did not answer.".into());
    };
    let Ok(resp) = serde_json::from_slice::<Response>(&bytes) else {
        return Err("The browser helper did not answer.".into());
    };
    if resp.ok {
        let version = resp
            .payload
            .as_ref()
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Ok(version)
    } else {
        let msg = resp.error.map(|e| e.message).unwrap_or_default();
        Err(format!("ODM did not answer the browser helper: {msg}"))
    }
}

/// Runs `<host_exe> --register --app <app_exe>` (no console window), waits up to 10 s.
pub fn repair(host_exe: &Path, app_exe: &Path) -> Result<(), String> {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    if !host_exe.exists() {
        return Err(MISSING_HOST.into());
    }
    let mut child = command(host_exe)
        .arg("--register")
        .arg("--app")
        .arg(app_exe)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "The browser helper could not be started.".to_string())?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(s)) if s.success() => return Ok(()),
            Ok(Some(_)) => return Err("The registration did not succeed.".into()),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("The registration took too long.".into());
            }
            Err(_) => return Err("The registration did not succeed.".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: &str = r"C:\Apps\ODM\odm-host.exe";

    fn check(manifest: Option<&str>, file: Option<&'static str>) -> BrowserCheck {
        check_browser(
            "Chrome",
            manifest.map(String::from),
            |_| file.map(String::from),
            Path::new(HOST),
        )
    }

    #[test]
    fn check_browser_missing() {
        let c = check(None, None);
        assert_eq!(c.status, Check::Missing);
        assert_eq!(c.detail, "Not registered.");
        assert_eq!(c.browser, "Chrome");
    }

    #[test]
    fn check_browser_broken_when_unreadable_or_not_json() {
        for file in [None, Some("not json {"), Some("")] {
            let c = check(Some(r"C:\m.json"), file);
            assert_eq!(c.status, Check::Broken, "{file:?}");
            assert_eq!(c.detail, "The registration file is damaged.");
        }
    }

    #[test]
    fn check_browser_wrong_path() {
        let c = check(
            Some(r"C:\m.json"),
            Some(r#"{"path":"C:\\Other\\odm-host.exe"}"#),
        );
        assert_eq!(c.status, Check::WrongPath);
        assert_eq!(c.detail, "Registered for another copy of ODM.");
        // A manifest without a path is not this copy either.
        assert_eq!(
            check(Some(r"C:\m.json"), Some("{}")).status,
            Check::WrongPath
        );
    }

    #[test]
    fn check_browser_ok_ignores_case_and_slashes() {
        let c = check(
            Some(r"C:\m.json"),
            Some(r#"{"path":"c:/apps/odm/ODM-HOST.exe"}"#),
        );
        assert_eq!(c.status, Check::Ok);
        assert_eq!(c.detail, "Ready.");
    }

    #[test]
    fn contact_records_command_and_time() {
        let c = Contact::default();
        assert!(c.get().is_none());
        c.record("ADD_DOWNLOAD");
        let (t, cmd) = c.get().unwrap();
        assert_eq!(cmd, "ADD_DOWNLOAD");
        assert!(now_ms().abs_diff(t) < 1000);
    }

    #[test]
    fn self_test_reports_a_missing_helper() {
        let r = self_test(Path::new(r"C:\definitely\not\here\odm-host.exe"));
        assert_eq!(
            r,
            Err("The browser helper (odm-host.exe) is missing next to ODM.".to_string())
        );
    }

    #[test]
    fn repair_reports_a_missing_helper() {
        let r = repair(
            Path::new(r"C:\definitely\not\here\odm-host.exe"),
            Path::new(r"C:\x\odm.exe"),
        );
        assert_eq!(
            r,
            Err("The browser helper (odm-host.exe) is missing next to ODM.".to_string())
        );
    }
}
