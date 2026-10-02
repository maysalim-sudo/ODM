use std::path::Path;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;

const KEEP_FILES: usize = 7;

/// A non-blocking writer onto `dir/odm.<date>.log`, rotated daily.
pub fn file_writer(dir: &Path) -> std::io::Result<(NonBlocking, WorkerGuard)> {
    std::fs::create_dir_all(dir)?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("odm")
        .filename_suffix("log")
        .max_log_files(KEEP_FILES)
        .build(dir)
        .map_err(std::io::Error::other)?;
    Ok(tracing_appender::non_blocking(appender))
}

/// Crates that can log full URLs (with query strings) or headers at debug/trace.
const QUIET_TARGETS: [&str; 5] = ["hyper", "hyper_util", "reqwest", "rustls", "h2"];

/// Build the log filter from a `RUST_LOG`-style spec (default `info`). The HTTP stack is
/// capped at `warn` whatever the spec says, so credentials and query strings stay out of logs.
pub fn filter_from(spec: Option<&str>) -> EnvFilter {
    let mut filter = spec
        .and_then(|s| EnvFilter::try_new(s).ok())
        .unwrap_or_else(|| EnvFilter::new("info"));
    for target in QUIET_TARGETS {
        filter = filter.add_directive(format!("{target}=warn").parse().expect("static directive"));
    }
    filter
}

/// Install the global logger. Keep the returned guard alive until the app exits.
pub fn init(dir: &Path) -> std::io::Result<WorkerGuard> {
    let (writer, guard) = file_writer(dir)?;
    let filter = filter_from(std::env::var("RUST_LOG").ok().as_deref());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init()
        .map_err(std::io::Error::other)?;
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    #[test]
    fn http_stack_is_capped_at_warn_whatever_the_spec() {
        for spec in [
            Some("trace"),
            Some("debug,hyper=trace"),
            Some("reqwest=trace"),
            None,
        ] {
            let text = super::filter_from(spec).to_string();
            for t in ["hyper", "hyper_util", "reqwest", "rustls", "h2"] {
                assert!(text.contains(&format!("{t}=warn")), "{spec:?}: {text}");
                assert!(!text.contains(&format!("{t}=trace")), "{spec:?}: {text}");
            }
        }
        assert!(super::filter_from(Some("trace"))
            .to_string()
            .contains("trace"));
    }

    #[test]
    fn old_logs_are_pruned_to_seven_and_new_file_is_dated() {
        let d = tempfile::tempdir().unwrap();
        for day in 1..=9 {
            std::fs::write(
                d.path().join(format!("odm.2020-01-{day:02}.log")),
                "old
",
            )
            .unwrap();
        }
        {
            let (mut w, _guard) = super::file_writer(d.path()).unwrap();
            w.write_all(
                b"new
",
            )
            .unwrap();
        }
        let names: Vec<String> = std::fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.len() <= 7, "{} files kept: {names:?}", names.len());
        let fresh: Vec<&String> = names
            .iter()
            .filter(|n| !n.starts_with("odm.2020-"))
            .collect();
        assert_eq!(fresh.len(), 1, "{names:?}");
        let parts: Vec<&str> = fresh[0].split('.').collect();
        assert_eq!(parts.len(), 3, "{names:?}");
        assert_eq!((parts[0], parts[2]), ("odm", "log"));
        let date = parts[1].as_bytes();
        assert_eq!(date.len(), 10, "{names:?}");
        for (i, b) in date.iter().enumerate() {
            let ok = if i == 4 || i == 7 {
                *b == b'-'
            } else {
                b.is_ascii_digit()
            };
            assert!(ok, "bad date segment {:?}", parts[1]);
        }
    }

    #[test]
    fn file_writer_creates_a_dated_log_file() {
        let d = tempfile::tempdir().unwrap();
        {
            let (mut w, _guard) = super::file_writer(d.path()).unwrap();
            w.write_all(b"hello\n").unwrap();
        } // guard dropped: the background writer flushes
        let names: Vec<String> = std::fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
        assert!(
            names[0].starts_with("odm.") && names[0].ends_with(".log"),
            "{names:?}"
        );
        let text = std::fs::read_to_string(d.path().join(&names[0])).unwrap();
        assert_eq!(text, "hello\n");
    }
}
