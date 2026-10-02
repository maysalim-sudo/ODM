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

/// Install the global logger. Keep the returned guard alive until the app exits.
pub fn init(dir: &Path) -> std::io::Result<WorkerGuard> {
    let (writer, guard) = file_writer(dir)?;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
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
