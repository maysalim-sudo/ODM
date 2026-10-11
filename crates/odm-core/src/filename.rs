use percent_encoding::percent_decode_str;

const MAX_CHARS: usize = 200;
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Invisible Unicode format characters (bidi overrides, zero-width, BOM, ...)
/// that a hostile site can use to disguise a file name.
fn is_format_char(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{061C}' | '\u{180E}'
        | '\u{200B}'..='\u{200F}'
        | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{2069}'
        | '\u{FEFF}')
}

pub fn sanitize(name: &str) -> String {
    let replaced: String = name
        .chars()
        .filter(|c| !is_format_char(*c))
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = replaced.trim_matches(|c: char| c == '.' || c == ' ');
    if trimmed.is_empty() {
        return "download".into();
    }
    let mut out = trimmed.to_string();
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        out.insert(0, '_');
    }
    truncate_keep_ext(&out, MAX_CHARS)
}

fn truncate_keep_ext(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_string();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if name.len() - i <= 16 && i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let keep = max - ext.chars().count();
    let stem: String = stem.chars().take(keep).collect();
    format!("{stem}{ext}")
}

pub fn from_url(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let path = rest.split(['?', '#']).next()?;
    let (_, path) = path.split_once('/')?;
    let last = path.rsplit('/').next()?;
    if last.is_empty() {
        return None;
    }
    let decoded = percent_decode_str(last).decode_utf8_lossy();
    Some(sanitize(&decoded))
}

pub fn from_content_disposition(value: &str) -> Option<String> {
    let mut plain: Option<String> = None;
    let mut star: Option<String> = None;
    for part in value.split(';').map(str::trim) {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim();
        if k == "filename*" {
            // charset'lang'percent-encoded
            let mut it = v.splitn(3, '\'');
            let (_charset, _lang, enc) = (it.next(), it.next(), it.next());
            if let Some(enc) = enc {
                star = Some(
                    percent_decode_str(enc.trim_matches('"'))
                        .decode_utf8_lossy()
                        .into_owned(),
                );
            }
        } else if k == "filename" {
            plain = Some(v.trim_matches('"').to_string());
        }
    }
    star.or(plain)
        .filter(|s| !s.is_empty())
        .map(|s| sanitize(&s))
}

pub fn unique_name(name: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(name) {
        return name.to_string();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (1..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !exists(candidate))
        .expect("infinite iterator")
}

/// File types that run code when opened. The UI keeps the same list in `app/src/lib/model.ts`.
pub const RISKY_EXTENSIONS: [&str; 29] = [
    "exe",
    "msi",
    "bat",
    "cmd",
    "com",
    "scr",
    "ps1",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "wsh",
    "hta",
    "lnk",
    "reg",
    "cpl",
    "msc",
    "jar",
    "appx",
    "msix",
    "pif",
    "chm",
    "msp",
    "scf",
    "url",
    "application",
    "appinstaller",
    "settingcontent-ms",
];

/// True for a file that runs code when opened, judged by its extension only.
pub fn is_risky(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| RISKY_EXTENSIONS.iter().any(|r| ext.eq_ignore_ascii_case(r)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_illegal_chars_and_dots() {
        assert_eq!(
            sanitize(r#"a<b>c:d"e/f\g|h?i*j.txt"#),
            "a_b_c_d_e_f_g_h_i_j.txt"
        );
        assert_eq!(sanitize("  ..name.txt.. "), "name.txt");
        assert_eq!(sanitize("tab\tnew\nline.bin"), "tab_new_line.bin");
    }

    #[test]
    fn format_chars_are_stripped() {
        assert_eq!(sanitize("evil\u{202E}gpj.exe"), "evilgpj.exe");
        assert_eq!(sanitize("a\u{200B}b\u{200D}c\u{FEFF}.txt"), "abc.txt");
        assert_eq!(sanitize("\u{2066}x\u{2069}.pdf"), "x.pdf");
        assert_eq!(sanitize("caf\u{00E9}.txt"), "caf\u{00E9}.txt"); // ordinary letters stay
    }

    #[test]
    fn rejects_traversal_and_empty() {
        assert_eq!(sanitize(".."), "download");
        assert_eq!(sanitize(""), "download");
        assert_eq!(sanitize("../../etc/passwd"), "_.._etc_passwd");
    }

    #[test]
    fn reserved_windows_names_prefixed() {
        assert_eq!(sanitize("CON"), "_CON");
        assert_eq!(sanitize("nul.txt"), "_nul.txt");
        assert_eq!(sanitize("com1.log"), "_com1.log");
        assert_eq!(sanitize("console.txt"), "console.txt");
    }

    #[test]
    fn long_names_truncated_keep_extension() {
        let long = format!("{}.mkv", "a".repeat(300));
        let s = sanitize(&long);
        assert_eq!(s.chars().count(), 200);
        assert!(s.ends_with(".mkv"));
    }

    #[test]
    fn from_url_basic() {
        assert_eq!(
            from_url("https://x.com/a/b/file%20name.zip?token=1#f").as_deref(),
            Some("file name.zip")
        );
        assert_eq!(from_url("https://x.com/").as_deref(), None);
        assert_eq!(from_url("not a url").as_deref(), None);
    }

    #[test]
    fn disposition_plain_and_star() {
        assert_eq!(
            from_content_disposition(r#"attachment; filename="report.pdf""#).as_deref(),
            Some("report.pdf")
        );
        assert_eq!(
            from_content_disposition("attachment; filename=plain.txt").as_deref(),
            Some("plain.txt")
        );
        assert_eq!(
            from_content_disposition(
                r#"attachment; filename="fallback.txt"; filename*=UTF-8''na%C3%AFve%20file.txt"#
            )
            .as_deref(),
            Some("naïve file.txt")
        );
        assert_eq!(from_content_disposition("inline").as_deref(), None);
    }

    #[test]
    fn long_utf8_disposition_truncated_keeps_ext() {
        let enc = "%E6%97%A5".repeat(250);
        let v = format!("attachment; filename*=UTF-8''{enc}.mp4");
        let name = from_content_disposition(&v).unwrap();
        assert_eq!(name.chars().count(), 200);
        assert!(name.ends_with(".mp4"));
        assert!(name.starts_with('日'));
    }

    #[test]
    fn unique_name_increments() {
        let taken = ["x.bin", "x (1).bin"];
        assert_eq!(unique_name("x.bin", |n| taken.contains(&n)), "x (2).bin");
        assert_eq!(unique_name("y.bin", |n| taken.contains(&n)), "y.bin");
        assert_eq!(unique_name("noext", |n| n == "noext"), "noext (1)");
    }
}

#[cfg(test)]
mod risky_tests {
    use super::is_risky;

    #[test]
    fn risky_types_are_programs_and_scripts() {
        assert!(is_risky("setup.EXE"));
        assert!(is_risky("script.ps1"));
        assert!(is_risky(".exe"));
        assert!(!is_risky("a.tar.gz"));
        assert!(!is_risky("noext"));
        assert!(!is_risky("movie.mp4"));
        for name in [
            "a.pif",
            "help.chm",
            "patch.msp",
            "x.scf",
            "link.url",
            "app.application",
            "app.appinstaller",
            "x.settingcontent-ms",
        ] {
            assert!(is_risky(name), "{name}");
        }
    }

    /// The UI keeps its own copy (app/src/lib/model.ts); both must name the same types.
    #[test]
    fn risky_list_matches_the_ui() {
        let ts = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src/lib/model.ts"),
        )
        .expect("model.ts");
        let start = ts
            .find("RISKY_EXTENSIONS")
            .expect("RISKY_EXTENSIONS in model.ts");
        let quoted = ts[start..].split('"').nth(1).expect("the quoted list");
        let mut ui: Vec<&str> = quoted.split(' ').collect();
        ui.sort_unstable();
        let mut ours = super::RISKY_EXTENSIONS.to_vec();
        ours.sort_unstable();
        assert_eq!(ui, ours);
    }
}
