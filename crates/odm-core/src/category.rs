use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Video,
    Audio,
    Document,
    Compressed,
    Image,
    Program,
    General,
    /// Torrent downloads; never chosen from a file's extension or type.
    Torrent,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Self::Video,
        Self::Audio,
        Self::Document,
        Self::Compressed,
        Self::Image,
        Self::Program,
        Self::General,
        Self::Torrent,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Document => "document",
            Self::Compressed => "compressed",
            Self::Image => "image",
            Self::Program => "program",
            Self::General => "general",
            Self::Torrent => "torrent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }

    pub fn default_folder(&self) -> &'static str {
        match self {
            Self::Video => "Videos",
            Self::Audio => "Music",
            Self::Document => "Documents",
            Self::Compressed => "Compressed",
            Self::Image => "Images",
            Self::Program => "Programs",
            Self::General => "",
            Self::Torrent => "Torrents",
        }
    }
}

fn by_ext(ext: &str) -> Option<Category> {
    use Category::*;
    Some(match ext {
        "mp4" | "mkv" | "avi" | "webm" | "mov" | "flv" | "m4v" | "ts" | "m3u8" => Video,
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "wma" | "m4a" | "opus" => Audio,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "csv" | "epub" => {
            Document
        }
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" | "zst" => Compressed,
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "ico" | "tiff" => Image,
        "exe" | "msi" | "msix" | "appx" => Program,
        _ => return None,
    })
}

fn by_mime(mime: &str) -> Option<Category> {
    use Category::*;
    let m = mime
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    Some(match m.as_str() {
        s if s.starts_with("video/") => Video,
        s if s.starts_with("audio/") => Audio,
        s if s.starts_with("image/") => Image,
        "application/pdf" => Document,
        "application/zip"
        | "application/x-rar-compressed"
        | "application/vnd.rar"
        | "application/x-7z-compressed"
        | "application/gzip" => Compressed,
        "application/x-msdownload"
        | "application/x-msi"
        | "application/vnd.microsoft.portable-executable" => Program,
        "application/vnd.apple.mpegurl" | "application/x-mpegurl" => Video,
        _ => return None,
    })
}

pub fn detect(filename: &str, mime: Option<&str>) -> Category {
    let ext = filename
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase());
    ext.as_deref()
        .and_then(by_ext)
        .or_else(|| mime.and_then(by_mime))
        .unwrap_or(Category::General)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_extension() {
        assert_eq!(detect("a.MP4", None), Category::Video);
        assert_eq!(detect("a.flac", None), Category::Audio);
        assert_eq!(detect("a.pdf", None), Category::Document);
        assert_eq!(detect("a.tar.gz", None), Category::Compressed);
        assert_eq!(detect("a.webp", None), Category::Image);
        assert_eq!(detect("setup.msi", None), Category::Program);
        assert_eq!(detect("a.xyz", None), Category::General);
    }

    #[test]
    fn by_mime_when_no_known_ext() {
        assert_eq!(detect("stream", Some("video/mp2t")), Category::Video);
        assert_eq!(detect("x", Some("application/zip")), Category::Compressed);
        assert_eq!(
            detect("x", Some("application/x-msdownload")),
            Category::Program
        );
        assert_eq!(detect("a.pdf", Some("video/mp4")), Category::Document); // extension wins
    }

    #[test]
    fn folders_and_parse() {
        assert_eq!(Category::Video.default_folder(), "Videos");
        assert_eq!(Category::General.default_folder(), "");
        assert_eq!(Category::parse("program"), Some(Category::Program));
    }

    #[test]
    fn torrent_category_round_trips() {
        assert_eq!(Category::parse("torrent"), Some(Category::Torrent));
        assert_eq!(Category::Torrent.as_str(), "torrent");
        assert_eq!(Category::Torrent.default_folder(), "Torrents");
        assert!(Category::ALL.contains(&Category::Torrent));
    }

    #[test]
    fn torrent_not_by_ext() {
        // A .torrent file downloaded over HTTP is a general file, not a torrent download.
        assert_eq!(detect("a.torrent", Some("application/x-bittorrent")), Category::General);
    }
}
