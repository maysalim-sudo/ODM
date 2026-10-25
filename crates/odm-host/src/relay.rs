use odm_core::ipc::{
    code, read_frame, skip_bytes, starts_app, write_frame, FrameError, Request, Response, MAX_FRAME,
};
#[cfg(windows)]
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[cfg(windows)]
const ERROR_FILE_NOT_FOUND: i32 = 2;
#[cfg(windows)]
const ERROR_PIPE_BUSY: i32 = 231;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x8;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x200;
#[cfg(windows)]
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
const START_FAILED: &str = "ODM could not be started. Open ODM once to set up browser integration.";

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub pipe_name: String,
    pub app_path: Option<PathBuf>,
    pub start_wait: Duration,
    pub reply_timeout: Duration,
}

impl HostConfig {
    /// ODM_PIPE_NAME, else the per-user pipe or socket; ODM_APP_PATH, else the registered AppPath
    /// (HKCU\Software\ODM on Windows, the registration file on Linux), for the app.
    /// An empty ODM_APP_PATH means "explicitly none" and skips the registration.
    pub fn from_env() -> Self {
        let pipe_name = odm_core::ids::pipe_name_from_env();
        let app_path = match std::env::var("ODM_APP_PATH") {
            Ok(v) if v.is_empty() => None,
            Ok(v) => Some(PathBuf::from(v)),
            Err(_) => app_path_from_registry(),
        };
        HostConfig {
            pipe_name,
            app_path,
            start_wait: Duration::from_secs(10),
            reply_timeout: Duration::from_secs(10),
        }
    }
}

#[cfg(windows)]
fn app_path_from_registry() -> Option<PathBuf> {
    let key = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(r"Software\ODM")
        .ok()?;
    let v: String = key.get_value("AppPath").ok()?;
    if v.is_empty() {
        None
    } else {
        Some(PathBuf::from(v))
    }
}

#[cfg(unix)]
fn app_path_from_registry() -> Option<PathBuf> {
    let home = odm_core::ids::home_dir()?;
    odm_core::ids::read_registration(&home, "AppPath")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(windows)]
fn spawn_app(path: &PathBuf) -> std::io::Result<()> {
    let attempt = |flags: u32| {
        Command::new(path)
            .arg("--background")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(flags)
            .spawn()
            .map(|_| ())
    };
    attempt(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| attempt(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP))
}

/// Starts the app in its own process group, so it outlives the browser stopping this helper.
#[cfg(unix)]
fn spawn_app(path: &PathBuf) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    Command::new(path)
        .arg("--background")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(|_| ())
}

#[cfg(windows)]
type Conn = File;
#[cfg(unix)]
type Conn = UnixStream;

/// Nobody is listening: the pipe does not exist, or (Unix) the socket file is left over.
fn is_not_running(e: &std::io::Error) -> bool {
    #[cfg(windows)]
    {
        e.raw_os_error() == Some(ERROR_FILE_NOT_FOUND)
    }
    #[cfg(unix)]
    {
        matches!(
            e.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
        )
    }
}

#[cfg(unix)]
fn open_pipe(name: &str) -> std::io::Result<Conn> {
    UnixStream::connect(name)
}

/// Opens the pipe, retrying every 50 ms (up to 2 s) while it is busy.
#[cfg(windows)]
fn open_pipe(name: &str) -> std::io::Result<Conn> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match OpenOptions::new().read(true).write(true).open(name) {
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            other => return other,
        }
    }
}

fn connect(id: &Option<String>, cmd: &str, cfg: &HostConfig) -> Result<Conn, Response> {
    let fail = |c: &str, m: &str| Response::err(id.clone(), c, m);
    match open_pipe(&cfg.pipe_name) {
        Ok(f) => Ok(f),
        Err(e) if is_not_running(&e) => {
            if !starts_app(cmd) {
                return Err(fail(code::APP_NOT_RUNNING, "ODM is not running."));
            }
            let Some(app) = &cfg.app_path else {
                return Err(fail(code::APP_START_FAILED, START_FAILED));
            };
            if spawn_app(app).is_err() {
                return Err(fail(code::APP_START_FAILED, START_FAILED));
            }
            let deadline = Instant::now() + cfg.start_wait;
            loop {
                match open_pipe(&cfg.pipe_name) {
                    Ok(f) => return Ok(f),
                    Err(e) if is_not_running(&e) && Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(200));
                    }
                    Err(_) => return Err(fail(code::APP_START_FAILED, START_FAILED)),
                }
            }
        }
        Err(_) => Err(fail(code::PIPE_ERROR, "Could not talk to ODM.")),
    }
}

/// Forwards one frame body; returns the bytes to write back to the browser.
fn relay_one(body: Vec<u8>, id: Option<String>, cmd: &str, cfg: &HostConfig) -> Vec<u8> {
    let mut pipe = match connect(&id, cmd, cfg) {
        Ok(f) => f,
        Err(r) => return r.to_bytes(),
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = write_frame(&mut pipe, &body)
            .map_err(|_| ())
            .and_then(|_| read_frame(&mut pipe).map_err(|_| ()));
        let _ = tx.send(result);
    });
    match rx.recv_timeout(cfg.reply_timeout) {
        Ok(Ok(bytes))
            if bytes.len() <= MAX_FRAME
                && serde_json::from_slice::<serde_json::Value>(&bytes).is_ok() =>
        {
            bytes
        }
        Ok(_) => Response::err(id, code::PIPE_ERROR, "ODM sent an unreadable reply.").to_bytes(),
        Err(_) => Response::err(id, code::TIMEOUT, "ODM did not answer in time.").to_bytes(),
    }
}

/// Reads frames from `input` until EOF; writes exactly one reply frame per request frame.
pub fn run<R: Read, W: Write>(
    input: &mut R,
    output: &mut W,
    cfg: &HostConfig,
) -> std::io::Result<()> {
    let reply = |out: &mut W, r: Response| write_frame(out, &r.to_bytes());
    loop {
        let body = match read_frame(input) {
            Ok(b) => b,
            Err(FrameError::Eof) => return Ok(()),
            Err(FrameError::TooLarge(n)) => {
                skip_bytes(input, n)?;
                let r = Response::err(None, code::TOO_LARGE, "Message is larger than 1 MiB.");
                reply(output, r)?;
                continue;
            }
            Err(FrameError::Truncated) => {
                let r = Response::err(None, code::BAD_MESSAGE, "Message was cut short.");
                reply(output, r)?;
                return Ok(());
            }
            Err(FrameError::Io(e)) => return Err(e),
        };
        let req: Request = match serde_json::from_slice(&body) {
            Ok(r) => r,
            Err(_) => {
                let r = Response::err(None, code::BAD_MESSAGE, "Message is not valid JSON.");
                reply(output, r)?;
                continue;
            }
        };
        let out = relay_one(body, req.id, &req.cmd, cfg);
        write_frame(output, &out)?;
    }
}
