//! Which command-line argument opens a torrent: Windows passes a magnet link (protocol
//! handler) or a `.torrent` path (file association) after the exe.
use odm_app_lib::torrent_arg::torrent_arg;
use odm_appcore::torrent::TorrentInput;

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn torrent_arg_magnet() {
    let m = "magnet:?xt=urn:btih:abcdef0123456789abcdef0123456789abcdef01";
    assert_eq!(
        torrent_arg(&args(&["ODM.exe", m])),
        Some(TorrentInput::Text(m.into()))
    );
    let upper = "MAGNET:?xt=urn:btih:abcdef0123456789abcdef0123456789abcdef01";
    assert_eq!(
        torrent_arg(&args(&["ODM.exe", upper])),
        Some(TorrentInput::Text(upper.into()))
    );
}

#[test]
fn torrent_arg_file() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("Some Show.TORRENT");
    std::fs::write(&p, b"x").unwrap();
    let s = p.to_string_lossy().into_owned();
    assert_eq!(
        torrent_arg(&args(&["ODM.exe", &s])),
        Some(TorrentInput::File(p))
    );
}

#[test]
fn torrent_arg_ignores_background_flag() {
    assert_eq!(torrent_arg(&args(&["ODM.exe", "--background"])), None);
    assert_eq!(torrent_arg(&args(&["ODM.exe"])), None);
    // The exe itself is never taken, even if its name looked like one.
    assert_eq!(
        torrent_arg(&args(&[
            "magnet:?xt=urn:btih:abcdef0123456789abcdef0123456789abcdef01"
        ])),
        None
    );
}

#[test]
fn torrent_arg_missing_file_is_none() {
    assert_eq!(
        torrent_arg(&args(&["ODM.exe", "C:/nowhere/gone.torrent"])),
        None
    );
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("notes.txt");
    std::fs::write(&p, b"x").unwrap();
    assert_eq!(torrent_arg(&args(&["ODM.exe", &p.to_string_lossy()])), None);
}
