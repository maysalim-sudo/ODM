//! Reading magnet links and `.torrent` files without touching the network.
use crate::{InfoHash, ParsedTorrent, TorrentError, TorrentFile, TorrentMeta, TorrentSource};
use bytes::Bytes;
use librqbit::{torrent_from_bytes, Magnet};

const NOT_VALID: &str = "This isn't a valid magnet link or torrent file.";

fn invalid() -> TorrentError {
    TorrentError::Invalid(NOT_VALID.into())
}

pub fn parse(source: &TorrentSource) -> Result<ParsedTorrent, TorrentError> {
    match source {
        TorrentSource::Magnet(uri) => parse_magnet(uri),
        TorrentSource::Bytes(b) => parse_bytes(b),
    }
}

/// The v1 info hash of a magnet link, or None when it is not one.
pub fn magnet_info_hash(uri: &str) -> Option<InfoHash> {
    parse_magnet(uri).ok().map(|p| p.info_hash)
}

fn parse_magnet(uri: &str) -> Result<ParsedTorrent, TorrentError> {
    let is_magnet = uri
        .get(..7)
        .is_some_and(|h| h.eq_ignore_ascii_case("magnet:"));
    if !is_magnet {
        return Err(invalid());
    }
    let m = Magnet::parse(uri).map_err(|_| invalid())?;
    // v2-only magnets (btmh without btih) are not supported.
    let id = m.as_id20().ok_or_else(invalid)?;
    Ok(ParsedTorrent {
        info_hash: InfoHash::parse(&id.as_string()).ok_or_else(invalid)?,
        name: m.name.clone(),
        trackers: m.trackers.clone(),
        meta: None,
    })
}

const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];

/// Whether Windows can create a file or folder with this name inside the output folder.
/// A drive-style name (`c:x`) would even lead outside it.
fn windows_ok(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." || name.ends_with(['.', ' ']) {
        return false;
    }
    if name
        .chars()
        .any(|c| c < ' ' || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
    {
        return false;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    let numbered = |p: &str| stem.len() == 4 && stem.starts_with(p) && stem.as_bytes()[3].is_ascii_digit();
    !(RESERVED.contains(&stem.as_str()) || numbered("COM") || numbered("LPT"))
}

/// Refuses a torrent with a path part Windows cannot use. librqbit writes the names as
/// they are, so such a torrent cannot be downloaded safely.
pub fn check_names(files: &[TorrentFile]) -> Result<(), TorrentError> {
    match files.iter().find(|f| !f.path.split('/').all(windows_ok)) {
        Some(f) => Err(TorrentError::Invalid(format!(
            "This torrent has a file name Windows can't use: {}",
            f.path
        ))),
        None => Ok(()),
    }
}

fn parse_bytes(bytes: &Bytes) -> Result<ParsedTorrent, TorrentError> {
    let t = torrent_from_bytes(bytes).map_err(|_| invalid())?;
    let trackers = t
        .iter_announce()
        .map(|a| String::from_utf8_lossy(a.as_ref()).into_owned())
        .collect();
    let info = t.info.data.clone().validate().map_err(|_| invalid())?;
    let name = info.name().map(|n| n.into_owned());
    let files = info
        .iter_file_details()
        .enumerate()
        .filter(|(_, d)| !d.attrs().padding)
        .map(|(idx, d)| TorrentFile {
            idx,
            path: d.filename.to_vec().join("/"),
            size: d.len,
        })
        .collect::<Vec<_>>();
    check_names(&files)?;
    Ok(ParsedTorrent {
        info_hash: InfoHash::parse(&t.info_hash.as_string()).ok_or_else(invalid)?,
        name: name.clone(),
        trackers,
        meta: Some(TorrentMeta {
            name: name.unwrap_or_else(|| "torrent".into()),
            files,
            torrent_bytes: bytes.clone(),
        }),
    })
}

/// `.torrent` bytes whose tracker list also holds `extra`. The info dict is kept byte for
/// byte, so the info hash does not change.
pub fn with_trackers(torrent: &Bytes, extra: &[String]) -> Result<Bytes, TorrentError> {
    let t = torrent_from_bytes(torrent).map_err(|_| invalid())?;
    let mut list: Vec<String> = t
        .iter_announce()
        .map(|a| String::from_utf8_lossy(a.as_ref()).into_owned())
        .collect();
    let before = list.len();
    for e in extra {
        if !list.contains(e) {
            list.push(e.clone());
        }
    }
    if list.len() == before {
        return Ok(torrent.clone());
    }
    fn bstr(out: &mut Vec<u8>, s: &[u8]) {
        out.extend_from_slice(s.len().to_string().as_bytes());
        out.push(b':');
        out.extend_from_slice(s);
    }
    // Keys in sorted order, as bencode requires. Other top-level keys (comment, dates) go.
    let mut out = b"d".to_vec();
    bstr(&mut out, b"announce");
    bstr(&mut out, list[0].as_bytes());
    bstr(&mut out, b"announce-list");
    out.push(b'l');
    for tr in &list {
        out.push(b'l');
        bstr(&mut out, tr.as_bytes());
        out.push(b'e');
    }
    out.push(b'e');
    bstr(&mut out, b"info");
    out.extend_from_slice(t.info.raw_bytes.as_ref());
    out.push(b'e');
    Ok(Bytes::from(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    const HEX: &str = "abcdef0123456789abcdef0123456789abcdef01";

    fn invalid(r: Result<ParsedTorrent, TorrentError>) -> bool {
        matches!(r, Err(TorrentError::Invalid(_)))
    }

    #[test]
    fn magnet_hex_hash() {
        let uri = "magnet:?xt=urn:btih:ABCDEF0123456789ABCDEF0123456789ABCDEF01&dn=Foo&tr=udp%3A%2F%2Ft.example%3A80";
        let p = parse(&TorrentSource::Magnet(uri.into())).unwrap();
        assert_eq!(p.info_hash.as_str(), HEX);
        assert_eq!(p.name.as_deref(), Some("Foo"));
        assert_eq!(p.trackers, vec!["udp://t.example:80".to_string()]);
        assert!(p.meta.is_none());
        assert_eq!(magnet_info_hash(uri).unwrap().as_str(), HEX);
    }

    #[test]
    fn magnet_base32_hash() {
        let uri = "magnet:?xt=urn:btih:VPG66AJDIVTYTK6N54ASGRLHRGV433YB";
        let p = parse(&TorrentSource::Magnet(uri.into())).unwrap();
        assert_eq!(p.info_hash.as_str(), HEX);
    }

    #[test]
    fn magnet_without_btih_is_invalid() {
        assert!(invalid(parse(&TorrentSource::Magnet("magnet:?dn=Foo".into()))));
    }

    #[test]
    fn not_a_magnet_is_invalid() {
        assert!(invalid(parse(&TorrentSource::Magnet("http://x/y".into()))));
        assert!(magnet_info_hash("http://x/y").is_none());
    }

    /// Two files plus a BEP 47 padding file between them, 16 KiB pieces, one piece.
    fn two_file_torrent() -> (Vec<u8>, Vec<u8>) {
        let mut info = Vec::new();
        info.extend_from_slice(b"d5:filesl");
        info.extend_from_slice(b"d6:lengthi3e4:pathl1:a5:x.txtee");
        info.extend_from_slice(b"d4:attr1:p6:lengthi5e4:pathl4:.pad1:5ee");
        info.extend_from_slice(b"d6:lengthi5e4:pathl5:y.binee");
        info.extend_from_slice(b"e4:name3:Foo12:piece lengthi16384e6:pieces20:");
        info.extend_from_slice(&[7u8; 20]);
        info.extend_from_slice(b"e");
        let mut t = b"d8:announce18:udp://t.example:804:info".to_vec();
        t.extend_from_slice(&info);
        t.extend_from_slice(b"e");
        (t, info)
    }

    #[test]
    fn torrent_bytes_lists_files() {
        use sha1::Digest;
        let (bytes, info) = two_file_torrent();
        let p = parse(&TorrentSource::Bytes(Bytes::from(bytes.clone()))).unwrap();
        let want_hash: String = sha1::Sha1::digest(&info)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(p.info_hash.as_str(), want_hash);
        assert_eq!(p.name.as_deref(), Some("Foo"));
        assert_eq!(p.trackers, vec!["udp://t.example:80".to_string()]);
        let meta = p.meta.unwrap();
        assert_eq!(meta.name, "Foo");
        let files: Vec<_> = meta.files.iter().map(|f| (f.idx, f.path.as_str(), f.size)).collect();
        assert_eq!(files, vec![(0, "a/x.txt", 3), (2, "y.bin", 5)]);
        assert_eq!(meta.torrent_bytes.as_ref(), bytes.as_slice());
    }

    #[test]
    fn with_trackers_keeps_hash_and_adds_trackers() {
        let (bytes, _) = two_file_torrent();
        let before = parse(&TorrentSource::Bytes(Bytes::from(bytes.clone()))).unwrap();
        let extra = vec!["udp://b.example:81".to_string(), "udp://t.example:80".to_string()];
        let out = with_trackers(&Bytes::from(bytes), &extra).unwrap();
        let after = parse(&TorrentSource::Bytes(out)).unwrap();
        assert_eq!(after.info_hash, before.info_hash);
        assert_eq!(
            after.trackers,
            vec!["udp://t.example:80".to_string(), "udp://b.example:81".to_string()]
        );
        assert_eq!(after.meta.unwrap().files, before.meta.unwrap().files);
    }

    #[test]
    fn with_no_extra_trackers_is_unchanged() {
        let (bytes, _) = two_file_torrent();
        let b = Bytes::from(bytes);
        assert_eq!(with_trackers(&b, &[]).unwrap(), b);
    }

    /// A one-piece multi-file torrent with these path components, written by hand: a real
    /// seeder on Windows can never produce names Windows cannot create.
    fn torrent_with(paths: &[&[&str]]) -> Bytes {
        let mut t = b"d4:infod5:filesl".to_vec();
        for p in paths {
            t.extend_from_slice(b"d6:lengthi1e4:pathl");
            for c in *p {
                t.extend_from_slice(format!("{}:{}", c.len(), c).as_bytes());
            }
            t.extend_from_slice(b"ee");
        }
        t.extend_from_slice(b"e4:name4:Pack12:piece lengthi16384e6:pieces20:");
        t.extend_from_slice(&[7u8; 20]);
        t.extend_from_slice(b"ee");
        Bytes::from(t)
    }

    #[test]
    fn names_windows_cannot_use_are_refused() {
        for bad in [
            &["c:x"][..],
            &["C:version.dll"],
            &["a:b"],
            &["CON"],
            &["sub", "aux.txt"],
            &["nul.tar.gz"],
            &["COM1"],
            &["what?"],
            &["trailing."],
            &["trailing "],
            &["tab	here"],
        ] {
            let r = parse(&TorrentSource::Bytes(torrent_with(&[&["ok.txt"], bad])));
            match r {
                Err(TorrentError::Invalid(m)) => {
                    assert!(m.starts_with("This torrent has a file name Windows can't use: "), "{m}");
                    assert!(m.contains(bad.last().unwrap().trim_end()), "{m}");
                }
                other => panic!("{bad:?} was accepted: {other:?}"),
            }
        }
    }

    #[test]
    fn ordinary_names_are_fine() {
        let ok = torrent_with(&[&["Season 1", "e01 - Pilot.mkv"], &["icon.txt"], &["CONSOLE.txt"], &["com10.log"], &["über ñ.txt"]]);
        let files = parse(&TorrentSource::Bytes(ok)).unwrap().meta.unwrap().files;
        assert_eq!(files.len(), 5);
    }

    #[test]
    fn garbage_bytes_are_invalid() {
        assert!(invalid(parse(&TorrentSource::Bytes(Bytes::from_static(b"not bencode")))));
    }
}
