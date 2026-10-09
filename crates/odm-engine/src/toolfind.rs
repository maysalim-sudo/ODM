//! Where the external tools (FFmpeg, yt-dlp, Deno) are looked for.
//!
//! Order, for every tool: the configured path, ODM's own tools folder, then the system
//! (`PATH`, the WinGet links folder, WinGet package folders).
use std::path::{Path, PathBuf};

/// `PATH` entries, then the WinGet `Links` folder, then every WinGet package folder whose
/// name starts with `package_prefix` (for example `yt-dlp.yt-dlp_`).
pub fn system_dirs(package_prefix: Option<&str>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let winget = PathBuf::from(local).join("Microsoft").join("WinGet");
        dirs.push(winget.join("Links"));
        if let (Some(prefix), Ok(rd)) = (package_prefix, std::fs::read_dir(winget.join("Packages")))
        {
            let mut pkgs: Vec<PathBuf> = rd
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
                .map(|e| e.path())
                .collect();
            pkgs.sort();
            dirs.extend(pkgs);
        }
    }
    dirs
}

/// The first existing `exe`: the configured file, `tools_dir/exe`, then each of `system`.
pub fn find_tool_in(
    configured: Option<&Path>,
    tools_dir: Option<&Path>,
    exe: &str,
    system: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(p) = configured.filter(|p| p.is_file()) {
        return Some(p.to_path_buf());
    }
    if let Some(p) = tools_dir.map(|d| d.join(exe)).filter(|p| p.is_file()) {
        return Some(p);
    }
    system.iter().map(|d| d.join(exe)).find(|c| c.is_file())
}

/// `find_tool_in` against the real system. With `search_system` off only the configured file and
/// the tools folder count.
pub fn find_tool(
    configured: Option<&Path>,
    tools_dir: Option<&Path>,
    exe: &str,
    search_system: bool,
    package_prefix: Option<&str>,
) -> Option<PathBuf> {
    let system = if search_system {
        system_dirs(package_prefix)
    } else {
        Vec::new()
    };
    find_tool_in(configured, tools_dir, exe, &system)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, b"x").unwrap();
        p
    }

    #[test]
    fn the_configured_path_beats_the_tools_dir() {
        let t = tempfile::tempdir().unwrap();
        let configured = touch(&t.path().join("cfg"), "my-ffmpeg.exe");
        touch(&t.path().join("tools"), "ffmpeg.exe");
        touch(&t.path().join("sys"), "ffmpeg.exe");
        let got = find_tool_in(
            Some(&configured),
            Some(&t.path().join("tools")),
            "ffmpeg.exe",
            &[t.path().join("sys")],
        );
        assert_eq!(got, Some(configured));
    }

    #[test]
    fn the_tools_dir_beats_path() {
        let t = tempfile::tempdir().unwrap();
        let managed = touch(&t.path().join("tools"), "yt-dlp.exe");
        touch(&t.path().join("sys"), "yt-dlp.exe");
        let got = find_tool_in(
            None,
            Some(&t.path().join("tools")),
            "yt-dlp.exe",
            &[t.path().join("sys")],
        );
        assert_eq!(got, Some(managed));
    }

    #[test]
    fn a_missing_configured_file_falls_through_to_the_tools_dir() {
        let t = tempfile::tempdir().unwrap();
        let managed = touch(&t.path().join("tools"), "yt-dlp.exe");
        let got = find_tool_in(
            Some(Path::new("Z:/nope/yt-dlp.exe")),
            Some(&t.path().join("tools")),
            "yt-dlp.exe",
            &[],
        );
        assert_eq!(got, Some(managed));
    }

    #[test]
    fn the_system_is_the_last_resort() {
        let t = tempfile::tempdir().unwrap();
        let sys = touch(&t.path().join("sys"), "ffmpeg.exe");
        let got = find_tool_in(
            None,
            Some(&t.path().join("empty-tools")),
            "ffmpeg.exe",
            &[t.path().join("other"), t.path().join("sys")],
        );
        assert_eq!(got, Some(sys));
        assert_eq!(find_tool_in(None, None, "ffmpeg.exe", &[]), None);
    }

    #[test]
    fn searching_off_ignores_the_system_but_not_the_tools_dir() {
        let t = tempfile::tempdir().unwrap();
        let managed = touch(&t.path().join("tools"), "deno.exe");
        let got = find_tool(None, Some(&t.path().join("tools")), "deno.exe", false, None);
        assert_eq!(got, Some(managed));
        assert_eq!(
            find_tool(None, None, "definitely-not-a-real-tool.exe", false, None),
            None
        );
    }
}
