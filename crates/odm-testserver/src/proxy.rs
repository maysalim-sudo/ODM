//! A tiny HTTP proxy for tests: plain requests in absolute form, and `CONNECT` tunnels.
//! Every request goes to one upstream address whatever host it names, so a test can use a
//! host that only this proxy knows and prove the proxy carried the request.
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// One request the proxy saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub method: String,
    /// The absolute URL of a plain request, or `host:port` of a `CONNECT`.
    pub target: String,
    pub user_agent: Option<String>,
}

pub struct TestProxy {
    pub addr: SocketAddr,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl TestProxy {
    /// Starts a proxy that sends everything to `upstream` (`http://127.0.0.1:port`).
    pub async fn start(upstream: &str) -> TestProxy {
        let up: SocketAddr = upstream
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .parse()
            .expect("upstream address");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let log = seen.clone();
        tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                let log = log.clone();
                tokio::spawn(async move {
                    let _ = serve(client, up, log).await;
                });
            }
        });
        TestProxy { addr, seen }
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

/// Reads up to the end of the request head; returns the head and any bytes after it.
async fn read_head(s: &mut TcpStream) -> std::io::Result<Option<(String, Vec<u8>)>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let rest = buf.split_off(end + 4);
            return Ok(Some((String::from_utf8_lossy(&buf).into_owned(), rest)));
        }
        let n = s.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > 64 * 1024 {
            return Ok(None);
        }
    }
}

async fn serve(
    mut client: TcpStream,
    up: SocketAddr,
    log: Arc<Mutex<Vec<Seen>>>,
) -> std::io::Result<()> {
    let Some((head, rest)) = read_head(&mut client).await? else {
        return Ok(());
    };
    let mut lines = head.split("\r\n");
    let first = lines.next().unwrap_or_default();
    let mut parts = first.split(' ');
    let (method, target, version) = (
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or("HTTP/1.1").to_string(),
    );
    let headers: Vec<&str> = lines.filter(|l| !l.is_empty()).collect();
    let user_agent = headers.iter().find_map(|h| {
        let (k, v) = h.split_once(':')?;
        k.trim()
            .eq_ignore_ascii_case("user-agent")
            .then(|| v.trim().to_string())
    });
    log.lock().unwrap().push(Seen {
        method: method.clone(),
        target: target.clone(),
        user_agent,
    });
    let mut server = TcpStream::connect(up).await?;
    if method == "CONNECT" {
        client
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await?;
        server.write_all(&rest).await?;
    } else {
        // Absolute form to origin form; one request per connection keeps the log exact.
        let path = target
            .split_once("://")
            .and_then(|(_, r)| r.find('/').map(|i| r[i..].to_string()))
            .unwrap_or_else(|| "/".into());
        let mut out = format!("{method} {path} {version}\r\n");
        for h in &headers {
            let name = h.split(':').next().unwrap_or_default().trim();
            if name.eq_ignore_ascii_case("connection")
                || name.eq_ignore_ascii_case("proxy-connection")
            {
                continue;
            }
            out.push_str(h);
            out.push_str("\r\n");
        }
        out.push_str("Connection: close\r\n\r\n");
        server.write_all(out.as_bytes()).await?;
        server.write_all(&rest).await?;
    }
    tokio::io::copy_bidirectional(&mut client, &mut server).await?;
    Ok(())
}
