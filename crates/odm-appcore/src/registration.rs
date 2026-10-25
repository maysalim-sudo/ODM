//! Keeps the browser integration registered against the paths this copy of ODM runs from.
use std::path::{Path, PathBuf};

/// The app and browser-helper paths to register and check against. An AppImage runs from a
/// temporary mount that is gone once ODM quits, so for one (`appimage` is `$APPIMAGE`) the app is
/// the AppImage file itself and the helper is copied to `<data_dir>/bin`, where browsers can
/// still start it. Everything else registers where it runs.
pub fn stable_paths(
    app_exe: &Path,
    host_exe: &Path,
    appimage: Option<&Path>,
    data_dir: &Path,
) -> std::io::Result<(PathBuf, PathBuf)> {
    let Some(appimage) = appimage else {
        return Ok((app_exe.to_path_buf(), host_exe.to_path_buf()));
    };
    let name = host_exe.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "helper path has no name")
    })?;
    let dir = data_dir.join("bin");
    let stable = dir.join(name);
    let bytes = std::fs::read(host_exe)?;
    // Rewritten only when it changed, so a running helper is left alone.
    if std::fs::read(&stable).ok().as_deref() != Some(bytes.as_slice()) {
        std::fs::create_dir_all(&dir)?;
        let part = dir.join(format!("{}.part", name.to_string_lossy()));
        std::fs::write(&part, &bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&part, &stable)?;
    }
    Ok((appimage.to_path_buf(), stable))
}

/// [`stable_paths`] for this process: the running executable, the helper next to it, and
/// `$APPIMAGE` when ODM runs as an AppImage.
pub fn current_paths(data_dir: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
    let app = std::env::current_exe()?;
    let host = app.with_file_name(odm_core::ids::host_exe_name());
    let appimage = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|p| cfg!(unix) && p.is_file());
    stable_paths(&app, &host, appimage.as_deref(), data_dir)
}

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
    fn outside_an_appimage_the_running_paths_are_registered() {
        let d = tempfile::tempdir().unwrap();
        let (app, host) = (d.path().join("odm-app"), d.path().join("odm-host"));
        let got = stable_paths(&app, &host, None, &d.path().join("data")).unwrap();
        assert_eq!(got, (app, host));
        assert!(!d.path().join("data").exists());
    }

    #[test]
    fn an_appimage_registers_itself_and_a_lasting_copy_of_the_helper() {
        let d = tempfile::tempdir().unwrap();
        let mount = d.path().join(".mount_ODM-abc/usr/bin");
        std::fs::create_dir_all(&mount).unwrap();
        let (app, host) = (mount.join("ODM"), mount.join("odm-host"));
        std::fs::write(&host, b"helper v1").unwrap();
        let appimage = d.path().join("ODM-x86_64.AppImage");
        let data = d.path().join("data");

        let (got_app, got_host) = stable_paths(&app, &host, Some(&appimage), &data).unwrap();
        assert_eq!(got_app, appimage);
        assert_eq!(got_host, data.join("bin").join("odm-host"));
        assert_eq!(std::fs::read(&got_host).unwrap(), b"helper v1");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&got_host).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }

        // The mount is gone after ODM quits; the copy stays. A newer AppImage replaces it.
        std::fs::write(&host, b"helper v2").unwrap();
        stable_paths(&app, &host, Some(&appimage), &data).unwrap();
        assert_eq!(std::fs::read(&got_host).unwrap(), b"helper v2");
        assert!(!data.join("bin").join("odm-host.part").exists());
    }

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
