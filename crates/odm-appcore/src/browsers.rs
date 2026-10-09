//! Opening a browser's extensions page for the first-run setup.
use std::path::PathBuf;

/// How to open one browser's extensions page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub exe: &'static str,
    pub url: &'static str,
    /// Usual install folders, relative to `%ProgramFiles%`, `%ProgramFiles(x86)%` or `%LOCALAPPDATA%`.
    pub folders: &'static [&'static str],
}

const FIREFOX_PAGE: &str = "about:debugging#/runtime/this-firefox";

/// The browsers the welcome screen offers. `None` for anything else: the name is never run as a command.
pub fn launch_spec(browser: &str) -> Option<LaunchSpec> {
    let (exe, url, folders): (&'static str, &'static str, &'static [&'static str]) = match browser {
        "Chrome" => (
            "chrome.exe",
            "chrome://extensions",
            &[r"Google\Chrome\Application"],
        ),
        "Edge" => (
            "msedge.exe",
            "edge://extensions",
            &[r"Microsoft\Edge\Application"],
        ),
        "Brave" => (
            "brave.exe",
            "brave://extensions",
            &[r"BraveSoftware\Brave-Browser\Application"],
        ),
        "Vivaldi" => (
            "vivaldi.exe",
            "vivaldi://extensions",
            &[r"Vivaldi\Application"],
        ),
        "Opera" => (
            "opera.exe",
            "opera://extensions",
            &[r"Programs\Opera", "Opera"],
        ),
        "Firefox" => ("firefox.exe", FIREFOX_PAGE, &[r"Mozilla Firefox"]),
        "Floorp" => ("floorp.exe", FIREFOX_PAGE, &["Floorp", r"Programs\Floorp"]),
        _ => return None,
    };
    Some(LaunchSpec { exe, url, folders })
}

/// Registry key (under HKLM or HKCU) whose default value is the exe's full path.
pub fn app_paths_key(exe: &str) -> String {
    format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{exe}")
}

/// Every place the exe may be, for the given install roots.
pub fn candidate_paths(spec: &LaunchSpec, roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .flat_map(|root| {
            spec.folders
                .iter()
                .map(move |f| root.join(f).join(spec.exe))
        })
        .collect()
}

#[cfg(windows)]
fn registered_exe(exe: &str) -> Option<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let key = app_paths_key(exe);
    [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER]
        .into_iter()
        .filter_map(|hive| RegKey::predef(hive).open_subkey(&key).ok())
        .filter_map(|k| k.get_value::<String, _>("").ok())
        .map(|v| PathBuf::from(v.trim().trim_matches('"')))
        .find(|p| p.is_file())
}

#[cfg(not(windows))]
fn registered_exe(_exe: &str) -> Option<PathBuf> {
    None
}

fn find_exe(spec: &LaunchSpec) -> Option<PathBuf> {
    if let Some(p) = registered_exe(spec.exe) {
        return Some(p);
    }
    let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
    candidate_paths(spec, &roots)
        .into_iter()
        .find(|p| p.is_file())
}

/// Store pages ODM may open on the user's behalf.
const STORE_PREFIXES: [&str; 3] = [
    "https://chromewebstore.google.com/",
    "https://microsoftedge.microsoft.com/",
    "https://addons.mozilla.org/",
];

pub fn is_store_url(url: &str) -> bool {
    STORE_PREFIXES.iter().any(|p| url.starts_with(p))
}

fn launch(browser: &str, url: &str) -> Result<(), String> {
    let spec = launch_spec(browser).ok_or("Unknown browser.")?;
    let not_found = || {
        format!(
            "ODM could not find {browser} on this PC. Open its extensions page yourself and follow the steps."
        )
    };
    let exe = find_exe(&spec).ok_or_else(not_found)?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg(url);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS: the browser must not die with ODM or share its console.
        cmd.creation_flags(0x0000_0008);
    }
    cmd.spawn().map(|_| ()).map_err(|_| not_found())
}

/// Starts the browser on its extensions page (or brings that page up in the running one).
pub fn open_extensions_page(browser: &str) -> Result<(), String> {
    let spec = launch_spec(browser).ok_or("Unknown browser.")?;
    launch(browser, spec.url)
}

/// Opens a store listing in the chosen browser. Only the known stores are allowed.
pub fn open_store_page(browser: &str, url: &str) -> Result<(), String> {
    if !is_store_url(url) {
        return Err("That is not a store page ODM knows.".into());
    }
    launch(browser, url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn every_browser_maps_to_an_exe_and_page() {
        let cases = [
            ("Chrome", "chrome.exe", "chrome://extensions"),
            ("Edge", "msedge.exe", "edge://extensions"),
            ("Brave", "brave.exe", "brave://extensions"),
            ("Vivaldi", "vivaldi.exe", "vivaldi://extensions"),
            ("Opera", "opera.exe", "opera://extensions"),
            (
                "Firefox",
                "firefox.exe",
                "about:debugging#/runtime/this-firefox",
            ),
            (
                "Floorp",
                "floorp.exe",
                "about:debugging#/runtime/this-firefox",
            ),
        ];
        for (name, exe, url) in cases {
            let spec = launch_spec(name).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(spec.exe, exe, "{name}");
            assert_eq!(spec.url, url, "{name}");
        }
    }

    #[test]
    fn only_store_pages_may_be_opened() {
        for ok in [
            "https://chromewebstore.google.com/detail/abcdefghijklmnopabcdefghijklmnop",
            "https://microsoftedge.microsoft.com/addons/detail/xyz",
            "https://addons.mozilla.org/firefox/addon/odm/",
        ] {
            assert!(is_store_url(ok), "{ok}");
        }
        for bad in [
            "http://chromewebstore.google.com/detail/x",
            "https://chromewebstore.google.com.evil.example/detail/x",
            "https://evil.example/https://addons.mozilla.org/",
            "file:///C:/Windows/System32/calc.exe",
            "chromewebstore.google.com/detail/x",
            "",
        ] {
            assert!(!is_store_url(bad), "{bad}");
        }
    }

    #[test]
    fn unknown_browsers_are_refused() {
        assert!(launch_spec("Netscape").is_none());
        assert!(launch_spec("").is_none());
        assert!(launch_spec("chrome.exe --evil").is_none());
    }

    #[test]
    fn app_paths_key_names_the_exe() {
        assert_eq!(
            app_paths_key("chrome.exe"),
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\chrome.exe"
        );
    }

    #[test]
    fn candidates_cover_each_root_and_folder() {
        let spec = launch_spec("Chrome").unwrap();
        let roots = [
            PathBuf::from(r"C:\Program Files"),
            PathBuf::from(r"C:\Users\u\AppData\Local"),
        ];
        let c = candidate_paths(&spec, &roots);
        assert!(c.contains(&PathBuf::from(
            r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        )));
        assert!(c.contains(&PathBuf::from(
            r"C:\Users\u\AppData\Local\Google\Chrome\Application\chrome.exe"
        )));
        assert!(candidate_paths(&spec, &[]).is_empty());
    }

    #[test]
    fn opera_lives_under_local_programs() {
        let spec = launch_spec("Opera").unwrap();
        let c = candidate_paths(&spec, &[PathBuf::from(r"C:\L")]);
        assert!(c.contains(&PathBuf::from(r"C:\L\Programs\Opera\opera.exe")));
    }
}
