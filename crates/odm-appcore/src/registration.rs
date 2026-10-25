//! Keeps the browser integration registered against the paths this copy of ODM runs from.
use std::path::Path;

fn canon(s: &str) -> String {
    s.replace('/', "\\").to_lowercase()
}

/// True if HostPath or AppPath is missing or differs (case-insensitive, after canonical \ separators) from these paths.
pub fn registration_needed(
    host_exe: &Path,
    app_exe: &Path,
    read: impl Fn(&str) -> Option<String>,
) -> bool {
    let same = |key: &str, want: &Path| {
        read(key).is_some_and(|v| canon(&v) == canon(&want.to_string_lossy()))
    };
    !(same("HostPath", host_exe) && same("AppPath", app_exe))
}

/// Reads HKCU\Software\ODM; if needed, runs `<host_exe> --register --app <app_exe>` and logs the outcome.
#[cfg(windows)]
pub fn ensure_registered(host_exe: &Path, app_exe: &Path) {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\ODM")
        .ok();
    let read = |name: &str| {
        key.as_ref()
            .and_then(|k| k.get_value::<String, _>(name).ok())
    };
    if !registration_needed(host_exe, app_exe, read) {
        tracing::debug!("browser integration is already registered");
        return;
    }
    if !host_exe.exists() {
        tracing::warn!("browser integration needs odm-host.exe next to ODM");
        return;
    }
    let child = Command::new(host_exe)
        .arg("--register")
        .arg("--app")
        .arg(app_exe)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("could not run the browser registration: {e}");
            return;
        }
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                tracing::info!("browser integration registered");
                return;
            }
            Ok(Some(status)) => {
                tracing::warn!("browser registration failed: {status}");
                return;
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                tracing::warn!("browser registration timed out");
                return;
            }
            Err(e) => {
                tracing::warn!("browser registration check failed: {e}");
                return;
            }
        }
    }
}

/// Linux: registers at every start, so the manifests follow an AppImage that moved and cover
/// browsers installed since.
#[cfg(unix)]
pub fn ensure_registered(host_exe: &Path, app_exe: &Path) {
    match crate::integration::repair(host_exe, app_exe) {
        Ok(()) => tracing::info!("browser integration registered"),
        Err(e) => tracing::warn!("browser registration failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn registration_needed_cases() {
        let host = Path::new(r"C:\Apps\ODM\odm-host.exe");
        let app = Path::new(r"C:\Apps\ODM\odm-app.exe");
        let read = |host_v: Option<&'static str>, app_v: Option<&'static str>| {
            move |k: &str| match k {
                "HostPath" => host_v.map(String::from),
                "AppPath" => app_v.map(String::from),
                _ => None,
            }
        };
        assert!(!registration_needed(
            host,
            app,
            read(
                Some("c:/apps/odm/ODM-HOST.exe"),
                Some(r"C:\Apps\ODM\odm-app.exe")
            )
        ));
        assert!(registration_needed(
            host,
            app,
            read(None, Some(r"C:\Apps\ODM\odm-app.exe"))
        ));
        assert!(registration_needed(
            host,
            app,
            read(
                Some(r"C:\Apps\ODM\odm-host.exe"),
                Some(r"C:\Other\odm-app.exe")
            )
        ));
    }
}
