//! Parsing of yt-dlp's output lines.
use std::path::PathBuf;

/// One line of yt-dlp output, as far as ODM cares.
#[derive(Debug, PartialEq)]
pub enum Line {
    Progress {
        downloaded: u64,
        total: Option<u64>,
        speed: Option<u64>,
        eta: Option<u64>,
    },
    Merging,
    Output(PathBuf),
    Other,
}

/// A whole number from a yt-dlp field; floats are truncated, `NA` and `None` are unknown.
fn number(s: &str) -> Option<u64> {
    s.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v as u64)
}

pub fn parse_line(line: &str) -> Line {
    let line = line.trim_end();
    if let Some(rest) = line.strip_prefix("ODMP ") {
        let f: Vec<&str> = rest.split_whitespace().collect();
        if let [downloaded, total, estimate, speed, eta] = f[..] {
            return match number(downloaded) {
                Some(downloaded) => Line::Progress {
                    downloaded,
                    total: number(total).or_else(|| number(estimate)),
                    speed: number(speed),
                    eta: number(eta),
                },
                None => Line::Other,
            };
        }
        return Line::Other;
    }
    if let Some(path) = line.strip_prefix("ODMFILE ") {
        let path = path.trim();
        if !path.is_empty() {
            return Line::Output(PathBuf::from(path));
        }
    }
    if ["[Merger]", "[VideoConvertor]", "[FixupM3u8]"]
        .iter()
        .any(|p| line.starts_with(p))
    {
        return Line::Merging;
    }
    Line::Other
}

/// yt-dlp downloads the video and the audio one after the other and reports each from zero.
/// This adds a finished stream's bytes to the next one's, so the total only grows.
#[derive(Debug, Default)]
pub struct ByteCounter {
    base: u64,
    last: u64,
    last_total: Option<u64>,
}

impl ByteCounter {
    /// Returns the combined `(downloaded, total)` for one reported `(downloaded, total)`.
    pub fn update(&mut self, downloaded: u64, total: Option<u64>) -> (u64, Option<u64>) {
        if downloaded < self.last {
            self.base += self.last_total.unwrap_or(self.last);
        }
        self.last = downloaded;
        self.last_total = total;
        (self.base + downloaded, total.map(|t| self.base + t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn progress_with_total() {
        assert_eq!(
            parse_line("ODMP 1000 5000 NA 250.5 12"),
            Line::Progress {
                downloaded: 1000,
                total: Some(5000),
                speed: Some(250),
                eta: Some(12)
            }
        );
    }

    #[test]
    fn progress_uses_the_estimate_when_total_is_unknown() {
        assert_eq!(
            parse_line("ODMP 10 NA 9000.7 NA NA"),
            Line::Progress {
                downloaded: 10,
                total: Some(9000),
                speed: None,
                eta: None
            }
        );
    }

    #[test]
    fn progress_all_unknown() {
        assert_eq!(
            parse_line("ODMP 7 NA NA None NA"),
            Line::Progress {
                downloaded: 7,
                total: None,
                speed: None,
                eta: None
            }
        );
    }

    #[test]
    fn merger_lines_mean_merging() {
        assert_eq!(
            parse_line("[Merger] Merging formats into \"x.mp4\""),
            Line::Merging
        );
        assert_eq!(parse_line("[VideoConvertor] Converting"), Line::Merging);
        assert_eq!(parse_line("[FixupM3u8] Fixing"), Line::Merging);
    }

    #[test]
    fn output_path_keeps_spaces() {
        assert_eq!(
            parse_line("ODMFILE D:\\My Videos\\clip [abc].mp4"),
            Line::Output(PathBuf::from("D:\\My Videos\\clip [abc].mp4"))
        );
    }

    #[test]
    fn everything_else_is_other() {
        assert_eq!(parse_line("[youtube] Extracting URL"), Line::Other);
        assert_eq!(parse_line(""), Line::Other);
        assert_eq!(parse_line("ODMP garbage"), Line::Other);
    }

    #[test]
    fn byte_counter_adds_finished_streams() {
        let mut c = ByteCounter::default();
        assert_eq!(c.update(50, Some(200)), (50, Some(200)));
        assert_eq!(c.update(150, Some(200)), (150, Some(200)));
        assert_eq!(c.update(200, Some(200)), (200, Some(200)));
        assert_eq!(c.update(30, Some(100)), (230, Some(300)));
        assert_eq!(c.update(100, Some(100)), (300, Some(300)));
    }

    #[test]
    fn byte_counter_without_totals() {
        let mut c = ByteCounter::default();
        assert_eq!(c.update(40, None), (40, None));
        assert_eq!(c.update(10, None), (50, None));
    }
}
