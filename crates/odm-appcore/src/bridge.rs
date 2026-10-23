//! Turns the engine's event stream into what the UI consumes: list rows
//! on change, progress coalesced to a fixed rate, and a resync signal
//! when events were missed.
use odm_engine::{EngineEvent, ManagerHandle, ProgressSnapshot};
use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::time::MissedTickBehavior;

use crate::dto::{to_dto, DownloadDto, UiEvent};

pub async fn pump<L, Fut, F>(
    mut rx: broadcast::Receiver<EngineEvent>,
    lookup: L,
    interval: Duration,
    emit: F,
) where
    L: Fn(String) -> Fut,
    Fut: Future<Output = Option<DownloadDto>>,
    F: Fn(UiEvent),
{
    let mut pending: HashMap<String, ProgressSnapshot> = HashMap::new();
    let mut tick = tokio::time::interval(interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if !pending.is_empty() {
                    let mut items: Vec<ProgressSnapshot> = pending.drain().map(|(_, v)| v).collect();
                    items.sort_by(|a, b| a.id.cmp(&b.id));
                    emit(UiEvent::ProgressBatch { items });
                }
            }
            event = rx.recv() => match event {
                Ok(EngineEvent::Progress(p)) => {
                    pending.insert(p.id.clone(), p);
                }
                Ok(EngineEvent::Added { id }) | Ok(EngineEvent::State { id, .. }) => {
                    let latest = pending.remove(&id);
                    if let Some(mut download) = lookup(id).await {
                        // The store lags the live counter by up to a persist interval; while
                        // the download is active, never let the byte count step backwards.
                        if let (Some(p), true) = (latest, download.state.is_active()) {
                            download.bytes = download.bytes.max(p.bytes);
                        }
                        emit(UiEvent::Upsert { download: Box::new(download) });
                    }
                }
                Ok(EngineEvent::Removed { id }) => {
                    pending.remove(&id);
                    emit(UiEvent::Removed { id });
                }
                Ok(EngineEvent::ExistsPrompt { id, name }) => emit(UiEvent::ExistsPrompt { id, name }),
                // The name and size changed with the file list.
                Ok(EngineEvent::TorrentMetadata { id }) => {
                    if let Some(download) = lookup(id.clone()).await {
                        emit(UiEvent::Upsert { download: Box::new(download) });
                    }
                    emit(UiEvent::TorrentMetadata { id });
                }
                // The power module acts on it; the list has nothing to show.
                Ok(EngineEvent::QueueDone) => {}
                Err(RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "UI bridge lagged behind engine events; asking the UI to reload");
                    pending.clear();
                    emit(UiEvent::Resync);
                }
                Err(RecvError::Closed) => break,
            }
        }
    }
}

/// Bridge a running manager to `emit` until the manager goes away.
pub async fn run<F>(manager: ManagerHandle, interval: Duration, emit: F)
where
    F: Fn(UiEvent),
{
    let rx = manager.subscribe();
    let lookup = |id: String| {
        let m = manager.clone();
        async move { m.get(&id).await.ok().flatten().map(|(r, b)| to_dto(&r, b)) }
    };
    pump(rx, lookup, interval, emit).await;
}
