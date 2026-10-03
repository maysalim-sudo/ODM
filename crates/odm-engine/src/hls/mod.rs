//! HLS support: playlist parsing, segment decryption and the download run.
pub mod crypto;
pub mod playlist;
pub mod run;

use odm_core::filename;

/// Replaces characters a file name cannot hold with spaces and tidies the spacing,
/// so a page title like `Test: clip?` becomes `Test clip`.
fn title_to_stem(title: &str) -> String {
    let spaced: String = title
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            c => c,
        })
        .collect();
    let tidy = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    filename::sanitize(&tidy)
}

fn with_mp4(name: &str) -> String {
    let base = name
        .strip_suffix(".m3u8")
        .or_else(|| name.strip_suffix(".M3U8"))
        .unwrap_or(name);
    if base.to_ascii_lowercase().ends_with(".mp4") {
        base.to_string()
    } else {
        format!("{base}.mp4")
    }
}

/// The output file name of a stream: the requested name, else the page title, else
/// the playlist's file stem, else `video.mp4`. Always ends in `.mp4`.
pub fn output_name(
    requested: Option<&str>,
    page_title: Option<&str>,
    playlist_url: &str,
) -> String {
    if let Some(n) = requested.filter(|n| !n.trim().is_empty()) {
        return with_mp4(&filename::sanitize(n));
    }
    if let Some(t) = page_title.filter(|t| !t.trim().is_empty()) {
        return with_mp4(&title_to_stem(t));
    }
    if let Some(n) = filename::from_url(playlist_url) {
        let stem = n.rsplit_once('.').map(|(s, _)| s).unwrap_or(&n);
        if !stem.is_empty() && stem != "download" {
            return with_mp4(stem);
        }
    }
    "video.mp4".into()
}

#[cfg(test)]
mod tests {
    use super::output_name;

    #[test]
    fn output_name_precedence() {
        let u = "https://e.com/a/index.m3u8?x=1";
        assert_eq!(output_name(Some("My Film"), Some("T"), u), "My Film.mp4");
        assert_eq!(output_name(Some("a.mp4"), None, u), "a.mp4");
        assert_eq!(output_name(None, Some("Test: clip?"), u), "Test clip.mp4");
        assert_eq!(output_name(None, None, u), "index.mp4");
        assert_eq!(output_name(None, Some("  "), "https://e.com/"), "video.mp4");
    }
}
