//! The torrent a command line asks ODM to open: Windows passes a magnet link (protocol
//! handler) or a `.torrent` path (file association) after the exe.
use odm_appcore::torrent::TorrentInput;
use std::path::PathBuf;

fn starts_with_ci(s: &str, prefix: &str) -> bool {
    s.get(..prefix.len())
        .is_some_and(|h| h.eq_ignore_ascii_case(prefix))
}

/// The first argument after the exe that is a magnet link or an existing `.torrent` file.
pub fn torrent_arg(argv: &[String]) -> Option<TorrentInput> {
    argv.iter().skip(1).find_map(|a| {
        if starts_with_ci(a, "magnet:") {
            return Some(TorrentInput::Text(a.clone()));
        }
        let p = PathBuf::from(a);
        let is_torrent = p
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("torrent"));
        (is_torrent && p.is_file()).then_some(TorrentInput::File(p))
    })
}
