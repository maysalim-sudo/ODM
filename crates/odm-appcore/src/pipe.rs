//! Server for the browser host: a named pipe on Windows, a Unix socket on Linux. Only the
//! current user can connect.
use crate::ipc::{dispatch, IpcContext};
use odm_core::ipc::{code, frame_len, FrameError, Request, Response};
#[cfg(windows)]
use std::ffi::c_void;
use std::io;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
#[cfg(windows)]
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio_util::sync::CancellationToken;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
#[cfg(windows)]
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
#[cfg(windows)]
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// The name ODM listens on: ODM_PIPE_NAME if set and non-empty, else `\\.\pipe\odm-<USERNAME>`
/// on Windows and `$XDG_RUNTIME_DIR/odm.sock` (or `/tmp/odm-<uid>.sock`) on Linux.
pub fn pipe_name_from_env() -> String {
    odm_core::ids::pipe_name_from_env()
}

#[cfg(windows)]
fn wide_to_string(p: *const u16) -> String {
    // SAFETY: `p` is a NUL-terminated UTF-16 string returned by the OS.
    unsafe {
        let mut len = 0;
        while *p.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
    }
}

/// `D:P(A;;GA;;;<current user SID>)`. Errors if the token can't be read.
#[cfg(windows)]
pub fn current_user_sddl() -> io::Result<String> {
    // SAFETY: plain Win32 calls; every out-pointer is valid for its call and every handle and
    // allocation obtained here is released before returning.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let result = (|| {
            let mut needed = 0u32;
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
            if needed == 0 {
                return Err(io::Error::last_os_error());
            }
            // u64 elements keep the buffer aligned for TOKEN_USER.
            let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
            if GetTokenInformation(
                token,
                TokenUser,
                buf.as_mut_ptr().cast(),
                needed,
                &mut needed,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut sid_str: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut sid_str) == 0 {
                return Err(io::Error::last_os_error());
            }
            let sid = wide_to_string(sid_str);
            LocalFree(sid_str.cast());
            Ok(sid)
        })();
        CloseHandle(token);
        Ok(format!("D:P(A;;GA;;;{})", result?))
    }
}

/// Creates one pipe instance whose DACL grants access to the current user only.
#[cfg(windows)]
fn create_instance(name: &str, first: bool, sddl: &str) -> io::Result<NamedPipeServer> {
    let wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
    let mut descriptor: *mut c_void = std::ptr::null_mut();
    // SAFETY: `wide` is NUL-terminated; the descriptor is freed after the pipe is created, and
    // CreateNamedPipe copies it.
    unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let created = ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(
                name,
                (&mut attrs as *mut SECURITY_ATTRIBUTES).cast(),
            );
        LocalFree(descriptor);
        created
    }
}

async fn write_response<S: AsyncWrite + Unpin>(pipe: &mut S, resp: &Response) -> io::Result<()> {
    let body = resp.to_bytes();
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    pipe.write_all(&out).await
}

async fn client_loop<S: AsyncRead + AsyncWrite + Unpin>(mut pipe: S, ctx: Arc<IpcContext>) {
    loop {
        let mut header = [0u8; 4];
        if pipe.read_exact(&mut header).await.is_err() {
            return;
        }
        let len = match frame_len(header) {
            Ok(n) => n,
            Err(FrameError::TooLarge(_)) => {
                let resp = Response::err(None, code::TOO_LARGE, "Message is larger than 1 MB.");
                let _ = write_response(&mut pipe, &resp).await;
                let _ = pipe.flush().await;
                return;
            }
            Err(_) => return,
        };
        let mut body = vec![0u8; len];
        if pipe.read_exact(&mut body).await.is_err() {
            return;
        }
        let resp = match serde_json::from_slice::<Request>(&body) {
            Ok(req) => dispatch(&ctx, req).await,
            Err(_) => Response::err(None, code::BAD_MESSAGE, "Message is not valid JSON."),
        };
        if write_response(&mut pipe, &resp).await.is_err() {
            return;
        }
    }
}

/// The runtime closes a dropped pipe handle only after its pending overlapped connect has been
/// cancelled, so for a moment the name can still be opened. Wait (bounded) until it cannot, so
/// that a finished server task really means a closed pipe.
#[cfg(windows)]
async fn wait_until_gone(name: &str) {
    use tokio::net::windows::named_pipe::ClientOptions;
    for _ in 0..200 {
        if ClientOptions::new().open(name).is_err() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

/// Binds the first instance before returning, so a squatted name is an immediate Err.
/// Then serves clients concurrently until `stop` is cancelled.
#[cfg(windows)]
pub async fn serve(
    name: String,
    ctx: Arc<IpcContext>,
    stop: CancellationToken,
) -> io::Result<tokio::task::JoinHandle<()>> {
    let sddl = current_user_sddl()?;
    let mut server = create_instance(&name, true, &sddl)?;
    Ok(tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop.cancelled() => {
                    drop(server);
                    wait_until_gone(&name).await;
                    return;
                }
                connected = server.connect() => {
                    if let Err(e) = connected {
                        tracing::warn!("pipe connect failed: {e}");
                        return;
                    }
                }
            }
            let next = match create_instance(&name, false, &sddl) {
                Ok(n) => n,
                Err(e) => {
                    tracing::error!("cannot create the next pipe instance: {e}");
                    return;
                }
            };
            let client = std::mem::replace(&mut server, next);
            let (ctx, stop) = (ctx.clone(), stop.clone());
            tokio::spawn(async move {
                tokio::select! {
                    _ = stop.cancelled() => {}
                    _ = client_loop(client, ctx) => {}
                }
            });
        }
    }))
}

/// Binds the socket before returning. A socket another process still answers on is an
/// immediate Err; a file left over from a crash is replaced. Then serves clients concurrently
/// until `stop` is cancelled, and removes the socket file.
#[cfg(unix)]
pub async fn serve(
    name: String,
    ctx: Arc<IpcContext>,
    stop: CancellationToken,
) -> io::Result<tokio::task::JoinHandle<()>> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::path::PathBuf::from(&name);
    if std::os::unix::net::UnixStream::connect(&path).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "another copy of ODM is already listening",
        ));
    }
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let listener = tokio::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    // SAFETY: getuid has no preconditions and cannot fail.
    let me = unsafe { libc::getuid() };
    Ok(tokio::spawn(async move {
        loop {
            let client = tokio::select! {
                _ = stop.cancelled() => {
                    drop(listener);
                    let _ = std::fs::remove_file(&path);
                    return;
                }
                accepted = listener.accept() => match accepted {
                    Ok((client, _)) => client,
                    Err(e) => {
                        tracing::warn!("socket accept failed: {e}");
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        continue;
                    }
                },
            };
            if !client.peer_cred().is_ok_and(|c| c.uid() == me) {
                tracing::warn!("refused a connection from another user");
                continue;
            }
            let (ctx, stop) = (ctx.clone(), stop.clone());
            tokio::spawn(async move {
                tokio::select! {
                    _ = stop.cancelled() => {}
                    _ = client_loop(client, ctx) => {}
                }
            });
        }
    }))
}
