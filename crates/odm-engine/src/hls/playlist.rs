//! A small, pure m3u8 parser. Every URI in the result is absolute.
use reqwest::Url;

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub url: String,
    pub bandwidth: u64,
    pub resolution: Option<String>,
    pub codecs: Option<String>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum KeyMethod {
    None,
    Aes128,
    SampleAes,
    Other(String),
}
#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub method: KeyMethod,
    pub uri: Option<String>,
    pub iv: Option<[u8; 16]>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub url: String,
    pub duration_ms: u64,
    pub seq: u64,
    pub key: Option<Key>,
    pub byte_range: Option<(u64, Option<u64>)>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct MediaPlaylist {
    pub segments: Vec<Segment>,
    pub map_url: Option<String>,
    pub ended: bool,
    pub media_sequence: u64,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Playlist {
    Master(Vec<Variant>),
    Media(MediaPlaylist),
}
#[derive(Debug, PartialEq)]
pub struct ParseError(pub String);

fn err(m: &str) -> ParseError {
    ParseError(m.to_string())
}

/// Splits `KEY=value,KEY2="quoted, value"` into pairs; quotes are removed.
fn attributes(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    loop {
        let mut name = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c == ',' {
                break;
            }
            name.push(c);
            chars.next();
        }
        if chars.peek() == Some(&',') {
            chars.next();
            continue;
        }
        if chars.next().is_none() {
            break;
        }
        let mut val = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            for c in chars.by_ref() {
                if c == '"' {
                    break;
                }
                val.push(c);
            }
            if chars.peek() == Some(&',') {
                chars.next();
            }
        } else {
            while let Some(&c) = chars.peek() {
                chars.next();
                if c == ',' {
                    break;
                }
                val.push(c);
            }
        }
        out.push((name.trim().to_string(), val));
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

fn resolve(base: &Url, uri: &str) -> Result<String, ParseError> {
    base.join(uri.trim())
        .map(|u| u.to_string())
        .map_err(|_| ParseError(format!("bad URI in playlist: {uri}")))
}

fn parse_iv(s: &str) -> Option<[u8; 16]> {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    if hex.is_empty() || hex.len() > 32 || !hex.is_ascii() {
        return None;
    }
    let padded = format!("{hex:0>32}");
    let mut iv = [0u8; 16];
    for (i, b) in iv.iter_mut().enumerate() {
        *b = u8::from_str_radix(&padded[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(iv)
}

fn parse_key(attrs: &[(String, String)], base: &Url) -> Result<Option<Key>, ParseError> {
    let method = match attr(attrs, "METHOD").unwrap_or("NONE") {
        "NONE" => return Ok(None),
        "AES-128" => KeyMethod::Aes128,
        "SAMPLE-AES" => KeyMethod::SampleAes,
        other => KeyMethod::Other(other.to_string()),
    };
    let uri = match attr(attrs, "URI") {
        Some(u) => Some(resolve(base, u)?),
        None => None,
    };
    Ok(Some(Key {
        method,
        uri,
        iv: attr(attrs, "IV").and_then(parse_iv),
    }))
}

fn parse_byte_range(s: &str) -> Option<(u64, Option<u64>)> {
    match s.split_once('@') {
        Some((len, off)) => Some((len.parse().ok()?, Some(off.parse().ok()?))),
        None => Some((s.parse().ok()?, None)),
    }
}

/// `base` is the playlist's own URL; every URI is returned absolute.
pub fn parse(text: &str, base: &str) -> Result<Playlist, ParseError> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if !text.starts_with("#EXTM3U") {
        return Err(err("not an HLS playlist"));
    }
    let base = Url::parse(base).map_err(|_| err("bad playlist URL"))?;

    let mut variants = Vec::new();
    let mut pending_variant: Option<Vec<(String, String)>> = None;
    let mut segments = Vec::new();
    let mut map_url = None;
    let mut ended = false;
    let mut media_sequence = 0u64;
    let mut key: Option<Key> = None;
    let mut pending_duration: Option<u64> = None;
    let mut pending_range = None;

    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(rest) = line.strip_prefix('#') {
            if let Some(a) = rest.strip_prefix("EXT-X-STREAM-INF:") {
                pending_variant = Some(attributes(a));
            } else if let Some(v) = rest.strip_prefix("EXTINF:") {
                let secs = v.split(',').next().unwrap_or("").trim();
                let secs: f64 = secs.parse().map_err(|_| err("bad EXTINF duration"))?;
                pending_duration = Some((secs * 1000.0).round() as u64);
            } else if let Some(v) = rest.strip_prefix("EXT-X-MEDIA-SEQUENCE:") {
                media_sequence = v.trim().parse().unwrap_or(0);
            } else if let Some(a) = rest.strip_prefix("EXT-X-KEY:") {
                key = parse_key(&attributes(a), &base)?;
            } else if let Some(a) = rest.strip_prefix("EXT-X-MAP:") {
                if let Some(u) = attr(&attributes(a), "URI") {
                    map_url = Some(resolve(&base, u)?);
                }
            } else if let Some(v) = rest.strip_prefix("EXT-X-BYTERANGE:") {
                pending_range = parse_byte_range(v.trim());
            } else if rest.starts_with("EXT-X-ENDLIST") {
                ended = true;
            }
            continue;
        }
        // A URI line.
        if let Some(attrs) = pending_variant.take() {
            variants.push(Variant {
                url: resolve(&base, line)?,
                bandwidth: attr(&attrs, "BANDWIDTH")
                    .and_then(|b| b.parse().ok())
                    .unwrap_or(0),
                resolution: attr(&attrs, "RESOLUTION").map(str::to_string),
                codecs: attr(&attrs, "CODECS").map(str::to_string),
            });
        } else if let Some(duration_ms) = pending_duration.take() {
            segments.push(Segment {
                url: resolve(&base, line)?,
                duration_ms,
                seq: media_sequence + segments.len() as u64,
                key: key.clone(),
                byte_range: pending_range.take(),
            });
        }
    }

    if !variants.is_empty() {
        return Ok(Playlist::Master(variants));
    }
    Ok(Playlist::Media(MediaPlaylist {
        segments,
        map_url,
        ended,
        media_sequence,
    }))
}

/// Highest bandwidth; ties keep the first.
pub fn best_variant(v: &[Variant]) -> Option<&Variant> {
    let mut best: Option<&Variant> = None;
    for x in v {
        if best.is_none_or(|b| x.bandwidth > b.bandwidth) {
            best = Some(x);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://cdn.example/a/b/index.m3u8";

    fn media(text: &str) -> MediaPlaylist {
        match parse(text, BASE).unwrap() {
            Playlist::Media(m) => m,
            other => panic!("expected media, got {other:?}"),
        }
    }

    #[test]
    fn parses_master_with_quoted_codecs() {
        let t = "#EXTM3U\n\
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,CODECS=\"avc1.42e01e,mp4a.40.2\"\nlow.m3u8\n\
#EXT-X-STREAM-INF:BANDWIDTH=2500000,RESOLUTION=1280x720,CODECS=\"avc1.4d401f,mp4a.40.2\"\nhigh.m3u8\n";
        let Playlist::Master(v) = parse(t, BASE).unwrap() else {
            panic!("expected master")
        };
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].bandwidth, 800000);
        assert_eq!(v[1].bandwidth, 2500000);
        assert_eq!(v[1].resolution.as_deref(), Some("1280x720"));
        assert_eq!(v[1].codecs.as_deref(), Some("avc1.4d401f,mp4a.40.2"));
        assert_eq!(v[1].url, "https://cdn.example/a/b/high.m3u8");
        assert_eq!(best_variant(&v).unwrap().bandwidth, 2500000);
    }

    #[test]
    fn best_variant_ties_keep_first() {
        let mk = |u: &str| Variant {
            url: u.into(),
            bandwidth: 5,
            resolution: None,
            codecs: None,
        };
        let v = [mk("a"), mk("b")];
        assert_eq!(best_variant(&v).unwrap().url, "a");
        assert!(best_variant(&[]).is_none());
    }

    #[test]
    fn parses_media_with_aes_key_and_map() {
        let t = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-MEDIA-SEQUENCE:7\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"https://k.example/key.bin\"\n\
#EXT-X-MAP:URI=\"init.mp4\"\n\
#EXTINF:4.0,\ns1.m4s\n#EXTINF:4.0,\ns2.m4s\n#EXTINF:2.5,\ns3.m4s\n#EXT-X-ENDLIST\n";
        let m = media(t);
        let d: Vec<u64> = m.segments.iter().map(|s| s.duration_ms).collect();
        assert_eq!(d, [4000, 4000, 2500]);
        let seqs: Vec<u64> = m.segments.iter().map(|s| s.seq).collect();
        assert_eq!(seqs, [7, 8, 9]);
        let want = Key {
            method: KeyMethod::Aes128,
            uri: Some("https://k.example/key.bin".into()),
            iv: None,
        };
        assert!(m.segments.iter().all(|s| s.key.as_ref() == Some(&want)));
        assert_eq!(
            m.map_url.as_deref(),
            Some("https://cdn.example/a/b/init.mp4")
        );
        assert!(m.ended);
        assert_eq!(m.media_sequence, 7);
    }

    #[test]
    fn key_none_and_rotation() {
        let t = "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"a.bin\"\n\
#EXTINF:1,\n1.ts\n#EXTINF:1,\n2.ts\n\
#EXT-X-KEY:METHOD=NONE\n#EXTINF:1,\n3.ts\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"b.bin\",IV=0x1\n#EXTINF:1,\n4.ts\n#EXT-X-ENDLIST\n";
        let m = media(t);
        let a = m.segments[0].key.clone().unwrap();
        assert_eq!(a.uri.as_deref(), Some("https://cdn.example/a/b/a.bin"));
        assert_eq!(m.segments[1].key, Some(a));
        assert_eq!(m.segments[2].key, None);
        let b = m.segments[3].key.clone().unwrap();
        assert_eq!(b.uri.as_deref(), Some("https://cdn.example/a/b/b.bin"));
        let mut iv = [0u8; 16];
        iv[15] = 1;
        assert_eq!(b.iv, Some(iv));
    }

    #[test]
    fn relative_uris_resolve_against_their_playlist() {
        let t = "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n\
#EXT-X-MAP:URI=\"../c/init.mp4\"\n\
#EXTINF:1,\nseg1.ts\n#EXTINF:1,\n../c/seg2.ts\n#EXTINF:1,\n/root/seg3.ts\n\
#EXTINF:1,\nhttps://other/x.ts\n#EXT-X-ENDLIST\n";
        let m = media(t);
        let urls: Vec<&str> = m.segments.iter().map(|s| s.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://cdn.example/a/b/seg1.ts",
                "https://cdn.example/a/c/seg2.ts",
                "https://cdn.example/root/seg3.ts",
                "https://other/x.ts"
            ]
        );
        assert_eq!(
            m.segments[0].key.as_ref().unwrap().uri.as_deref(),
            Some("https://cdn.example/a/b/key.bin")
        );
        assert_eq!(
            m.map_url.as_deref(),
            Some("https://cdn.example/a/c/init.mp4")
        );

        let master = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\n../v/m.m3u8\n\
#EXT-X-STREAM-INF:BANDWIDTH=2\n/abs/m.m3u8\n";
        let Playlist::Master(v) = parse(master, BASE).unwrap() else {
            panic!()
        };
        assert_eq!(v[0].url, "https://cdn.example/a/v/m.m3u8");
        assert_eq!(v[1].url, "https://cdn.example/abs/m.m3u8");
    }

    #[test]
    fn live_playlist_has_no_endlist() {
        let m = media("#EXTM3U\n#EXTINF:4,\n1.ts\n#EXTINF:4,\n2.ts\n");
        assert!(!m.ended);
        assert_eq!(m.segments.len(), 2);
    }

    #[test]
    fn rejects_non_playlist() {
        assert_eq!(
            parse("<html>", BASE),
            Err(ParseError("not an HLS playlist".into()))
        );
    }

    #[test]
    fn crlf_and_bom_accepted() {
        let t = "\u{feff}  \r\n#EXTM3U\r\n#EXT-X-MEDIA-SEQUENCE:3\r\n#EXTINF:2.0,\r\ns.ts\r\n#EXT-X-ENDLIST\r\n";
        let m = media(t);
        assert_eq!(m.segments.len(), 1);
        assert_eq!(m.segments[0].url, "https://cdn.example/a/b/s.ts");
        assert_eq!(m.segments[0].seq, 3);
        assert!(m.ended);
    }

    #[test]
    fn byte_range_and_unknown_tags() {
        let t = "#EXTM3U\n#EXT-X-FOO:1\n#EXT-X-BYTERANGE:100@50\n#EXTINF:1,\nx.ts\n\
#EXTINF:1,\ny.ts\n#EXT-X-ENDLIST\n";
        let m = media(t);
        assert_eq!(m.segments[0].byte_range, Some((100, Some(50))));
        assert_eq!(m.segments[1].byte_range, None);
    }
}
