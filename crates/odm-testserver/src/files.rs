//! A tiny in-memory file server: fixed bodies at fixed paths, optionally cut off mid-body.
//! Used by the video-tools installer tests.
use axum::{
    body::Body,
    extract::{Path as UrlPath, State},
    http::{header, Response},
    routing::get,
    Router,
};
use bytes::Bytes;
use futures_util::stream;
use std::collections::HashMap;
use std::sync::Arc;

struct Inner {
    files: HashMap<String, Vec<u8>>,
    /// Paths whose body stops halfway even though `Content-Length` promises all of it.
    cut_off: Vec<String>,
}

pub struct FileServer {
    pub base: String,
}

impl FileServer {
    /// `files` maps a path without the leading slash (`"tool/yt-dlp.exe"`) to its body.
    /// Any path in `cut_off` is answered with a body that breaks off halfway.
    pub async fn start(files: Vec<(String, Vec<u8>)>, cut_off: Vec<String>) -> FileServer {
        let inner = Arc::new(Inner {
            files: files.into_iter().collect(),
            cut_off,
        });
        let app = Router::new()
            .route("/{*path}", get(serve))
            .with_state(inner);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        FileServer {
            base: format!("http://{addr}"),
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base, path.trim_start_matches('/'))
    }
}

async fn serve(State(s): State<Arc<Inner>>, UrlPath(path): UrlPath<String>) -> Response<Body> {
    let Some(data) = s.files.get(&path) else {
        return Response::builder().status(404).body(Body::empty()).unwrap();
    };
    let b = Response::builder()
        .status(200)
        .header(header::CONTENT_LENGTH, data.len().to_string());
    if s.cut_off.contains(&path) {
        let first = Bytes::copy_from_slice(&data[..data.len() / 2]);
        let st = stream::iter([
            Ok::<_, std::io::Error>(first),
            Err(std::io::Error::other("cut off")),
        ]);
        return b.body(Body::from_stream(st)).unwrap();
    }
    b.body(Body::from(data.clone())).unwrap()
}
