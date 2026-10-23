//! A scriptable [`TorrentClient`] for engine and app tests. No network, no disk.
use crate::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct FakeScript {
    pub meta: Option<TorrentMeta>,
    /// How long a magnet's metadata takes; None = never (until cancelled).
    pub metadata_after: Option<Duration>,
    pub fail_start: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FakeCall {
    Fetch(InfoHash),
    /// A metadata fetch ended because its token was cancelled.
    FetchCancelled(InfoHash),
    Start { hash: InfoHash, only_files: Option<Vec<usize>> },
    SetFiles(InfoHash, Vec<usize>),
    Pause(InfoHash),
    Remove { hash: InfoHash, delete: bool },
    ApplySettings(TorrentSettings),
}

#[derive(Default)]
struct Entry {
    script: FakeScript,
    live: bool,
    only_files: Option<Vec<usize>>,
    done: u64,
}

#[derive(Default)]
struct State {
    torrents: HashMap<InfoHash, Entry>,
    calls: Vec<FakeCall>,
}

#[derive(Default)]
pub struct FakeTorrentClient {
    state: Mutex<State>,
}

impl FakeTorrentClient {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// How the torrent with `hash` behaves; replaces an earlier script.
    pub fn script(&self, hash: &InfoHash, s: FakeScript) {
        self.lock().torrents.entry(hash.clone()).or_default().script = s;
    }

    pub fn calls(&self) -> Vec<FakeCall> {
        self.lock().calls.clone()
    }

    /// Torrents currently in the "session", sorted.
    pub fn live(&self) -> Vec<InfoHash> {
        let mut v: Vec<_> = self
            .lock()
            .torrents
            .iter()
            .filter(|(_, e)| e.live)
            .map(|(h, _)| h.clone())
            .collect();
        v.sort();
        v
    }

    /// Sets the selected bytes done that the next `stats` reports.
    pub fn advance(&self, hash: &InfoHash, done: u64) {
        self.lock().torrents.entry(hash.clone()).or_default().done = done;
    }
}

fn selected_total(e: &Entry) -> u64 {
    let Some(meta) = &e.script.meta else { return 0 };
    meta.files
        .iter()
        .filter(|f| e.only_files.as_ref().is_none_or(|o| o.contains(&f.idx)))
        .map(|f| f.size)
        .sum()
}

#[async_trait]
impl TorrentClient for FakeTorrentClient {
    async fn fetch_metadata(
        &self,
        source: &TorrentSource,
        cancel: CancellationToken,
    ) -> Result<TorrentMeta, TorrentError> {
        let parsed = parse(source)?;
        if let Some(meta) = parsed.meta {
            return Ok(meta);
        }
        let hash = parsed.info_hash;
        let script = {
            let mut st = self.lock();
            st.calls.push(FakeCall::Fetch(hash.clone()));
            st.torrents.get(&hash).map(|e| e.script.clone()).unwrap_or_default()
        };
        let wait = async {
            match script.metadata_after {
                Some(after) => tokio::time::sleep(after).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            _ = cancel.cancelled() => {
                self.lock().calls.push(FakeCall::FetchCancelled(hash));
                Err(TorrentError::Cancelled)
            }
            _ = wait => script.meta.ok_or_else(|| TorrentError::Invalid("no metadata scripted".into())),
        }
    }

    async fn start(&self, req: StartTorrent) -> Result<(), TorrentError> {
        let mut st = self.lock();
        st.calls.push(FakeCall::Start { hash: req.info_hash.clone(), only_files: req.only_files.clone() });
        let e = st.torrents.entry(req.info_hash).or_default();
        if let Some(msg) = &e.script.fail_start {
            return Err(TorrentError::Io(msg.clone()));
        }
        e.live = true;
        e.only_files = req.only_files;
        Ok(())
    }

    async fn set_files(&self, hash: &InfoHash, selected: &[usize]) -> Result<(), TorrentError> {
        let mut st = self.lock();
        st.calls.push(FakeCall::SetFiles(hash.clone(), selected.to_vec()));
        let e = st.torrents.get_mut(hash).filter(|e| e.live).ok_or(TorrentError::NotFound)?;
        e.only_files = Some(selected.to_vec());
        Ok(())
    }

    async fn pause(&self, hash: &InfoHash) -> Result<(), TorrentError> {
        self.lock().calls.push(FakeCall::Pause(hash.clone()));
        Ok(())
    }

    async fn remove(&self, hash: &InfoHash, delete_files: bool) -> Result<(), TorrentError> {
        let mut st = self.lock();
        st.calls.push(FakeCall::Remove { hash: hash.clone(), delete: delete_files });
        if let Some(e) = st.torrents.get_mut(hash) {
            e.live = false;
        }
        Ok(())
    }

    fn stats(&self, hash: &InfoHash) -> Option<TorrentStats> {
        let st = self.lock();
        let e = st.torrents.get(hash).filter(|e| e.live)?;
        let total = selected_total(e);
        let done = e.done.min(total);
        let has_meta = e.script.meta.is_some();
        Some(TorrentStats {
            selected_total: total,
            selected_done: done,
            down_bps: 1000,
            up_bps: 10,
            peers_connected: 3,
            peers_seen: 7,
            files: Vec::new(),
            finished: has_meta && total > 0 && done >= total,
            checking: false,
            has_meta,
            error: None,
        })
    }

    fn pieces(&self, hash: &InfoHash) -> Option<PieceMap> {
        self.stats(hash).map(|_| PieceMap { piece_count: 8, have: vec![0b1010_0000] })
    }

    async fn apply_settings(&self, settings: TorrentSettings) -> Result<(), TorrentError> {
        self.lock().calls.push(FakeCall::ApplySettings(settings));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    const HEX: &str = "abcdef0123456789abcdef0123456789abcdef01";

    fn hash() -> InfoHash {
        InfoHash::parse(HEX).unwrap()
    }

    fn magnet() -> TorrentSource {
        TorrentSource::Magnet(format!("magnet:?xt=urn:btih:{HEX}&dn=Foo"))
    }

    fn meta() -> TorrentMeta {
        TorrentMeta {
            name: "Foo".into(),
            files: vec![
                TorrentFile { idx: 0, path: "a".into(), size: 100 },
                TorrentFile { idx: 1, path: "b".into(), size: 50 },
            ],
            torrent_bytes: Bytes::new(),
        }
    }

    fn start(only: Option<Vec<usize>>) -> StartTorrent {
        StartTorrent {
            info_hash: hash(),
            source: magnet(),
            save_dir: "x".into(),
            only_files: only,
            trackers: vec![],
            download_bps: None,
        }
    }

    #[tokio::test]
    async fn fake_records_calls() {
        let f = FakeTorrentClient::new();
        f.script(&hash(), FakeScript { meta: Some(meta()), ..Default::default() });
        f.start(start(Some(vec![1]))).await.unwrap();
        f.set_files(&hash(), &[0, 1]).await.unwrap();
        f.pause(&hash()).await.unwrap();
        f.remove(&hash(), true).await.unwrap();
        assert_eq!(
            f.calls(),
            vec![
                FakeCall::Start { hash: hash(), only_files: Some(vec![1]) },
                FakeCall::SetFiles(hash(), vec![0, 1]),
                FakeCall::Pause(hash()),
                FakeCall::Remove { hash: hash(), delete: true },
            ]
        );
        assert!(f.live().is_empty());
    }

    #[tokio::test]
    async fn fake_metadata_never_until_cancel() {
        let f = FakeTorrentClient::new();
        f.script(&hash(), FakeScript { meta: Some(meta()), metadata_after: None, ..Default::default() });
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            c2.cancel();
        });
        assert_eq!(f.fetch_metadata(&magnet(), cancel).await, Err(TorrentError::Cancelled));

        f.script(&hash(), FakeScript { meta: Some(meta()), metadata_after: Some(Duration::ZERO), ..Default::default() });
        assert_eq!(f.fetch_metadata(&magnet(), CancellationToken::new()).await.unwrap(), meta());
    }

    #[tokio::test]
    async fn fake_finishes_when_done_reaches_total() {
        let f = FakeTorrentClient::new();
        f.script(&hash(), FakeScript { meta: Some(meta()), ..Default::default() });
        assert!(f.stats(&hash()).is_none());
        f.start(start(Some(vec![0]))).await.unwrap();
        f.advance(&hash(), 60);
        let st = f.stats(&hash()).unwrap();
        assert_eq!((st.selected_total, st.selected_done, st.finished), (100, 60, false));
        f.advance(&hash(), 100);
        assert!(f.stats(&hash()).unwrap().finished);
        assert_eq!(f.live(), vec![hash()]);
    }

    #[tokio::test]
    async fn fake_start_failure() {
        let f = FakeTorrentClient::new();
        f.script(&hash(), FakeScript { fail_start: Some("disk full".into()), ..Default::default() });
        assert_eq!(f.start(start(None)).await, Err(TorrentError::Io("disk full".into())));
        assert!(f.live().is_empty());
    }
}
