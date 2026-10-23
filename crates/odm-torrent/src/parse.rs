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
        .collect();
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
    fn garbage_bytes_are_invalid() {
        assert!(invalid(parse(&TorrentSource::Bytes(Bytes::from_static(b"not bencode")))));
    }
}
