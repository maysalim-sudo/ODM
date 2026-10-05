//! A stand-in for yt-dlp used by the engine tests. Not shipped.
//!
//! It writes `video [abc].mp4` (300 000 patterned bytes) into the directory of the `-o`
//! template and prints progress lines in the format ODM asks for.
//! `FAKE_YTDLP_FAIL=1` prints 25 lines to stderr and exits 1, `FAKE_YTDLP_SLOW=1` sleeps
//! 200 ms between lines, `FAKE_YTDLP_ARGS=<file>` writes argv (one per line) to that file.
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(file) = std::env::var_os("FAKE_YTDLP_ARGS") {
        let _ = std::fs::write(file, args.join("\n"));
    }
    let slow = std::env::var_os("FAKE_YTDLP_SLOW").is_some();

    if std::env::var_os("FAKE_YTDLP_FAIL").is_some() {
        for i in 1..=25 {
            eprintln!("ERROR line {i}");
        }
        std::process::exit(1);
    }

    let template = args
        .iter()
        .position(|a| a == "-o")
        .and_then(|i| args.get(i + 1))
        .expect("-o template");
    let dir = PathBuf::from(template)
        .parent()
        .expect("template directory")
        .to_path_buf();
    std::fs::create_dir_all(&dir).expect("create output dir");
    let out = dir.join("video [abc].mp4");
    let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(&out, data).expect("write output");

    let mut stdout = std::io::stdout();
    let mut line = |s: String| {
        let _ = writeln!(stdout, "{s}");
        let _ = stdout.flush();
        if slow {
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    for d in [50_000, 100_000, 200_000] {
        line(format!("ODMP {d} 200000 NA 1000.5 3"));
    }
    for d in [50_000, 100_000] {
        line(format!("ODMP {d} 100000 NA 1000.5 1"));
    }
    line("[Merger] Merging formats into \"video [abc].mp4\"".into());
    line(format!("ODMFILE {}", out.display()));
}
