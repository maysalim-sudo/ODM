mod common;

use common::{fixture, req};
#[cfg(windows)]
use odm_appcore::pipe::current_user_sddl;
use odm_appcore::pipe::serve;
use odm_core::ipc::{cmd, code, Response, MAX_FRAME};
use serde_json::json;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio_util::sync::CancellationToken;

#[cfg(windows)]
type Client = NamedPipeClient;
#[cfg(unix)]
type Client = tokio::net::UnixStream;

#[cfg(windows)]
fn unique_name() -> String {
    format!(r"\\.\pipe\odm-test-{}", uuid::Uuid::new_v4())
}

#[cfg(unix)]
fn unique_name() -> String {
    std::env::temp_dir()
        .join(format!("odm-test-{}.sock", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned()
}

#[cfg(windows)]
fn open(name: &str) -> std::io::Result<Client> {
    ClientOptions::new().open(name)
}

#[cfg(unix)]
fn open(name: &str) -> std::io::Result<Client> {
    let s = std::os::unix::net::UnixStream::connect(name)?;
    s.set_nonblocking(true)?;
    Client::from_std(s)
}

async fn send(c: &mut Client, body: &[u8]) {
    c.write_all(&(body.len() as u32).to_le_bytes())
        .await
        .unwrap();
    c.write_all(body).await.unwrap();
}

async fn recv(c: &mut Client) -> Response {
    let mut h = [0u8; 4];
    c.read_exact(&mut h).await.unwrap();
    let mut body = vec![0u8; u32::from_le_bytes(h) as usize];
    c.read_exact(&mut body).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn ping_bytes() -> Vec<u8> {
    serde_json::to_vec(&req(cmd::PING, json!({}))).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn round_trip_ping() {
    let f = fixture().await;
    let name = unique_name();
    let stop = CancellationToken::new();
    serve(name.clone(), f.ctx.clone(), stop.clone())
        .await
        .unwrap();
    let mut c = open(&name).unwrap();
    send(&mut c, br#"{"id":"a","cmd":"PING"}"#).await;
    let r = recv(&mut c).await;
    assert_eq!(r.id.as_deref(), Some("a"));
    assert!(r.ok);
    stop.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn show_window_reaches_the_context() {
    let f = fixture().await;
    let name = unique_name();
    let stop = CancellationToken::new();
    serve(name.clone(), f.ctx.clone(), stop.clone())
        .await
        .unwrap();
    let mut c = open(&name).unwrap();
    let body = serde_json::to_vec(&req(cmd::SHOW_WINDOW, json!({}))).unwrap();
    send(&mut c, &body).await;
    assert!(recv(&mut c).await.ok);
    assert_eq!(f.shows(), 1);
    assert_eq!(f.offers_seen(), 0);
    assert!(f.confirms().is_empty());
    stop.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn server_survives_garbage() {
    let f = fixture().await;
    let name = unique_name();
    let stop = CancellationToken::new();
    serve(name.clone(), f.ctx.clone(), stop.clone())
        .await
        .unwrap();

    let mut c1 = open(&name).unwrap();
    send(&mut c1, b"not json").await;
    let r = recv(&mut c1).await;
    assert!(!r.ok);
    assert_eq!(r.error.unwrap().code, code::BAD_MESSAGE);
    send(&mut c1, &ping_bytes()).await;
    assert!(recv(&mut c1).await.ok);

    let mut c2 = open(&name).unwrap();
    c2.write_all(&((MAX_FRAME + 1) as u32).to_le_bytes())
        .await
        .unwrap();
    let r = recv(&mut c2).await;
    assert_eq!(r.error.unwrap().code, code::TOO_LARGE);
    let mut rest = Vec::new();
    let n = tokio::time::timeout(Duration::from_secs(2), c2.read_to_end(&mut rest))
        .await
        .expect("connection should close");
    assert!(matches!(n, Ok(0)) || n.is_err());

    let mut c3 = open(&name).unwrap();
    send(&mut c3, &ping_bytes()).await;
    assert!(recv(&mut c3).await.ok);
    stop.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_clients() {
    let f = fixture().await;
    let name = unique_name();
    let stop = CancellationToken::new();
    serve(name.clone(), f.ctx.clone(), stop.clone())
        .await
        .unwrap();
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let name = name.clone();
        tasks.push(tokio::spawn(async move {
            let mut c = loop {
                match open(&name) {
                    Ok(c) => break c,
                    Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            };
            send(&mut c, &ping_bytes()).await;
            recv(&mut c).await.ok
        }));
    }
    for t in tasks {
        assert!(t.await.unwrap());
    }
    stop.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn second_server_on_same_name_fails() {
    let f = fixture().await;
    let name = unique_name();
    let stop = CancellationToken::new();
    serve(name.clone(), f.ctx.clone(), stop.clone())
        .await
        .unwrap();
    assert!(serve(name.clone(), f.ctx.clone(), stop.clone())
        .await
        .is_err());
    stop.cancel();
}

#[cfg(windows)]
#[test]
fn sddl_names_current_user() {
    let s = current_user_sddl().unwrap();
    assert!(s.starts_with("D:P(A;;GA;;;S-1-5-"), "{s}");
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_ends_the_server() {
    let f = fixture().await;
    let name = unique_name();
    let stop = CancellationToken::new();
    let handle = serve(name.clone(), f.ctx.clone(), stop.clone())
        .await
        .unwrap();
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("server should stop")
        .unwrap();
    assert!(open(&name).is_err());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_leftover_socket_file_is_replaced_and_removed_on_stop() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture().await;
    let name = unique_name();
    // A socket file nobody listens on, as a crash leaves behind.
    drop(std::os::unix::net::UnixListener::bind(&name).unwrap());
    assert!(std::path::Path::new(&name).exists());
    let stop = CancellationToken::new();
    let handle = serve(name.clone(), f.ctx.clone(), stop.clone())
        .await
        .unwrap();
    let mode = std::fs::metadata(&name).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    let mut c = open(&name).unwrap();
    send(&mut c, &ping_bytes()).await;
    assert!(recv(&mut c).await.ok);
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("server should stop")
        .unwrap();
    assert!(!std::path::Path::new(&name).exists());
}
