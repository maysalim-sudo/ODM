use odm_core::ipc::{read_frame, Response};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::ServerOptions;

fn unique_name() -> String {
    format!(r"\\.\pipe\odm-test-{}", uuid::Uuid::new_v4())
}

fn frame(body: &[u8]) -> Vec<u8> {
    let mut v = (body.len() as u32).to_le_bytes().to_vec();
    v.extend_from_slice(body);
    v
}

/// Echo server for `n` connections. The first pipe instance is created before returning,
/// so a client opening the pipe right after this call always finds it.
fn start_server(name: String, n: usize) -> std::thread::JoinHandle<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let first = {
        let _g = rt.enter();
        ServerOptions::new()
            .first_pipe_instance(true)
            .create(&name)
            .unwrap()
    };
    std::thread::spawn(move || {
        rt.block_on(async move {
            let mut first = Some(first);
            for _ in 0..n {
                let mut s = match first.take() {
                    Some(s) => s,
                    None => ServerOptions::new().create(&name).unwrap(),
                };
                s.connect().await.unwrap();
                let mut h = [0u8; 4];
                s.read_exact(&mut h).await.unwrap();
                let mut b = vec![0u8; u32::from_le_bytes(h) as usize];
                s.read_exact(&mut b).await.unwrap();
                let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
                let r =
                    serde_json::json!({"id": v["id"], "ok": true, "payload": {"echo": v["cmd"]}});
                let rb = serde_json::to_vec(&r).unwrap();
                s.write_all(&(rb.len() as u32).to_le_bytes()).await.unwrap();
                s.write_all(&rb).await.unwrap();
                s.flush().await.unwrap();
                let _ = s.read(&mut [0u8; 1]).await; // wait for the client to close
            }
        })
    })
}

fn run_host(
    pipe: &str,
    app_path: &str,
    input: Vec<u8>,
) -> (Vec<Response>, std::process::ExitStatus) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_odm-host"))
        .env("ODM_PIPE_NAME", pipe)
        .env("ODM_APP_PATH", app_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let mut out = child.stdout.take().unwrap();
    let mut replies = vec![];
    while let Ok(b) = read_frame(&mut out) {
        replies.push(serde_json::from_slice::<Response>(&b).unwrap());
    }
    let status = child.wait().unwrap();
    let _ = writer.join();
    (replies, status)
}

fn code_of(r: &Response) -> &str {
    &r.error.as_ref().unwrap().code
}

#[test]
fn forwards_and_returns_reply() {
    let name = unique_name();
    let srv = start_server(name.clone(), 1);
    let (r, st) = run_host(&name, "", frame(br#"{"id":"1","cmd":"PING"}"#));
    assert!(st.success());
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].id.as_deref(), Some("1"));
    assert!(r[0].ok);
    assert_eq!(r[0].payload.as_ref().unwrap()["echo"], "PING");
    srv.join().unwrap();
}

#[test]
fn host_replies_to_garbage_and_continues() {
    let name = unique_name();
    let srv = start_server(name.clone(), 1);
    let mut input = frame(b"nope");
    input.extend_from_slice(&(2u32 << 20).to_le_bytes());
    input.extend(std::iter::repeat_n(b'x', 2 << 20));
    input.extend(frame(br#"{"id":"p","cmd":"PING"}"#));
    let (r, st) = run_host(&name, "", input);
    assert!(st.success());
    assert_eq!(r.len(), 3);
    assert_eq!(code_of(&r[0]), "bad_message");
    assert_eq!(r[0].id, None);
    assert_eq!(code_of(&r[1]), "too_large");
    assert_eq!(r[1].id, None);
    assert!(r[2].ok);
    assert_eq!(r[2].payload.as_ref().unwrap()["echo"], "PING");
    srv.join().unwrap();
}

#[test]
fn no_app_for_ping_replies_not_running() {
    let name = unique_name();
    let (r, _) = run_host(&name, "", frame(br#"{"id":"1","cmd":"PING"}"#));
    assert_eq!(r.len(), 1);
    assert_eq!(code_of(&r[0]), "app_not_running");
    assert_eq!(r[0].id.as_deref(), Some("1"));
}

#[test]
fn missing_app_path_replies_start_failed() {
    let name = unique_name();
    let t = Instant::now();
    let (r, _) = run_host(
        &name,
        "",
        frame(br#"{"id":"1","cmd":"ADD_DOWNLOAD","payload":{"url":"http://x/"}}"#),
    );
    assert!(t.elapsed() < Duration::from_secs(1));
    assert_eq!(r.len(), 1);
    assert_eq!(code_of(&r[0]), "app_start_failed");
}

#[test]
fn starts_app_then_waits_for_pipe() {
    let name = unique_name();
    let whoami = format!(
        r"{}\System32\whoami.exe",
        std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())
    );
    let n2 = name.clone();
    let t = Instant::now();
    let starter = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(1));
        start_server(n2, 1).join().unwrap();
    });
    let (r, _) = run_host(&name, &whoami, frame(br#"{"id":"s","cmd":"SHOW_WINDOW"}"#));
    assert!(t.elapsed() >= Duration::from_millis(900));
    assert_eq!(r.len(), 1);
    assert!(r[0].ok, "{:?}", r[0]);
    assert_eq!(r[0].payload.as_ref().unwrap()["echo"], "SHOW_WINDOW");
    starter.join().unwrap();
}

#[test]
fn truncated_input_gets_a_reply() {
    let name = unique_name();
    let (r, st) = run_host(&name, "", vec![9, 0, 0, 0, b'{']);
    assert!(st.success());
    assert_eq!(r.len(), 1);
    assert_eq!(code_of(&r[0]), "bad_message");
}
