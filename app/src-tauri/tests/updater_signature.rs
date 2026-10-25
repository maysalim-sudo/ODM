//! The updater's signature check, end to end: the real updater plugin, configured the way ODM
//! configures it (`updates::updater_for`), against a local server.

use base64::Engine;
use odm_app_lib::updates::{updater_for, user_error, Stage, VERIFY_FAILED};
use odm_appcore::settings::ProxyMode;
use odm_appcore::Settings;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use tauri::Url;

// A throwaway key pair made with `tauri signer generate` in a temp folder outside the repo;
// only its public half and one signature over PAYLOAD are kept. It signs nothing real.
const TEST_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDVGMEYzMTBCNTk5RjM4RDEKUldUUk9KOVpDekVQWDFKblZYc0dNeWZ5Ylg4ZVlYREtCZ2dFTEZlQjZMWG1zcHl0WVdUdHpUQzIK";
const PAYLOAD: &[u8] = b"ODM update test payload 0.1.1\n";
const PAYLOAD_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUUk9KOVpDekVQWCt4THVhSmgvMTBwaGFsTFVIRE01cHhHY0w3WDdaMWZGZnF2c01lTmRTZzZicGxWOGtYTXRCamJCczM5N2JvRi9XMkJUVDQ4cnFaTHIxYS9wNmc4OEFrPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkxNjYxNjg0CWZpbGU6cGF5bG9hZC5iaW4KZXFIVDBZSTZnTWVHVW1IQWRqTyt4ekZrSzV2QkE0RG9qZ3pOclU1aUk4R0plUmZkQVljVFptR2UvUVY4M2JHZXpaei95c3RrU252TlRmUG1iSVpLRFE9PQo=";

type Files = Vec<(&'static str, Vec<u8>)>;

/// Serves `/<name>` over plain http until the test ends; `files` gets the server's address.
fn serve(files: impl FnOnce(&str) -> Files) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let files = files(&base);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                }
            }
            let line = String::from_utf8_lossy(&buf)
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            let path = line
                .split(' ')
                .nth(1)
                .unwrap_or("/")
                .trim_start_matches('/')
                .to_string();
            let found = files.iter().find(|(n, _)| *n == path);
            let (status, body) = match found {
                Some((_, b)) => ("200 OK", b.clone()),
                None => ("404 Not Found", Vec::new()),
            };
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(head.as_bytes());
            let _ = s.write_all(&body);
        }
    });
    base
}

/// One entry, listed for each platform the tests run on.
fn manifest(base: &str, payload: &str, signature: &str) -> Vec<u8> {
    let entry = json!({ "signature": signature, "url": format!("{base}/{payload}") });
    json!({
        "version": "0.1.1",
        "notes": "Fixes.",
        "pub_date": "2026-10-10T08:00:00Z",
        "platforms": { "windows-x86_64": entry, "linux-x86_64": entry }
    })
    .to_string()
    .into_bytes()
}

/// The signature with one character of the signature line changed.
fn tampered(sig_b64: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD;
    let text = String::from_utf8(b64.decode(sig_b64).unwrap()).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let sig = &mut lines[1];
    let c = if sig.as_bytes()[20] == b'A' { "B" } else { "A" };
    sig.replace_range(20..21, c);
    b64.encode(format!("{}\n", lines.join("\n")))
}

/// The real updater plugin, with ODM's updater settings, against a local server: a valid
/// signature verifies, a tampered signature or a changed installer does not.
#[test]
fn only_a_valid_signature_is_accepted() {
    let base = serve(|base| {
        vec![
            ("good.json", manifest(base, "setup.exe", PAYLOAD_SIG)),
            (
                "bad-sig.json",
                manifest(base, "setup.exe", &tampered(PAYLOAD_SIG)),
            ),
            ("bad-file.json", manifest(base, "changed.exe", PAYLOAD_SIG)),
            ("setup.exe", PAYLOAD.to_vec()),
            ("changed.exe", b"ODM update test payload 6.6.6\n".to_vec()),
        ]
    });

    let mut ctx = tauri::test::mock_context(tauri::test::noop_assets());
    ctx.config_mut().plugins.0.insert(
        "updater".into(),
        json!({ "pubkey": TEST_PUBKEY, "endpoints": [] }),
    );
    let app = tauri::test::mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(ctx)
        .unwrap();
    let settings = Settings {
        proxy_mode: ProxyMode::None,
        ..Settings::default()
    };
    let fetch = |name: &str| {
        let url = Url::parse(&format!("{base}/{name}")).unwrap();
        let updater = updater_for(app.handle(), &settings, Some(&url)).unwrap();
        tauri::async_runtime::block_on(async {
            let update = updater
                .check()
                .await
                .unwrap()
                .expect("0.1.1 is newer than 0.1.0");
            assert_eq!(update.version, "0.1.1");
            update.download(|_, _| {}, || {}).await
        })
    };

    assert_eq!(fetch("good.json").unwrap(), PAYLOAD);
    let e = fetch("bad-sig.json").unwrap_err();
    assert_eq!(user_error(Stage::Download, &e), VERIFY_FAILED, "{e}");
    assert_eq!(
        VERIFY_FAILED,
        "The update couldn't be verified, so it wasn't installed."
    );
    let e = fetch("bad-file.json").unwrap_err();
    assert_eq!(user_error(Stage::Download, &e), VERIFY_FAILED, "{e}");
}
