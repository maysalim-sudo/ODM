//! Turns a finished plain file or HLS stream into an MP3 with FFmpeg.
use odm_core::{ErrorKind, OdmError};
use odm_store::DownloadRecord;
use std::path::Path;
use tokio_util::sync::CancellationToken;

use crate::config::{apply_override, Quality, VideoOptions};
use crate::hls::merge::{run_ffmpeg, stderr_tail};

/// True when this download itself asked for audio. Only site downloads follow the Settings.
pub fn wants_mp3(rec: &DownloadRecord) -> bool {
    rec.video_override.is_some()
        && apply_override(&VideoOptions::default(), rec.video_override.as_deref()).quality
            == Quality::Audio
}

/// `clip.mp4` becomes `clip.mp3`.
pub fn mp3_name(name: &str) -> String {
    Path::new(name)
        .with_extension("mp3")
        .to_string_lossy()
        .into_owned()
}

fn failed(detail: &str) -> OdmError {
    OdmError::new(
        ErrorKind::Other,
        format!("FFmpeg could not convert this to MP3:\n{detail}"),
    )
}

/// Writes `output` from the audio of `input`. `input` is left alone; on failure `output` is gone.
pub async fn to_mp3(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
    cancel: &CancellationToken,
) -> Result<(), OdmError> {
    let (input_arg, output_arg) = (input.to_string_lossy(), output.to_string_lossy());
    let args = [
        "-hide_banner",
        "-y",
        "-i",
        input_arg.as_ref(),
        "-vn",
        "-c:a",
        "libmp3lame",
        "-q:a",
        "2",
        output_arg.as_ref(),
    ];
    let dir = input.parent().unwrap_or(Path::new("."));
    let result = run_ffmpeg(ffmpeg, &args, dir, cancel, failed).await;
    let error = match result {
        Ok((status, log)) if !status.success() || !output.is_file() => {
            failed(&stderr_tail(&log, 20))
        }
        Ok(_) => return Ok(()),
        Err(e) => e,
    };
    let _ = std::fs::remove_file(output);
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp3_names() {
        assert_eq!(mp3_name("clip.mp4"), "clip.mp3");
        assert_eq!(mp3_name("clip"), "clip.mp3");
        assert_eq!(mp3_name("a.b.webm"), "a.b.mp3");
    }

    #[test]
    fn only_a_download_that_asked_is_converted() {
        let mut rec = DownloadRecord::new_file("a", "https://e.com/f", "f", "D:/out");
        assert!(!wants_mp3(&rec));
        rec.video_override = Some(r#"{"quality":"audio"}"#.into());
        assert!(wants_mp3(&rec));
        rec.video_override = Some(r#"{"container":"mp4"}"#.into());
        assert!(!wants_mp3(&rec));
    }
}
