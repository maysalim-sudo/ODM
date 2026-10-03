//! Total download speed across active downloads, fed from manager events.
use odm_engine::{EngineEvent, ManagerHandle};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast::error::RecvError;

#[derive(Default)]
pub struct SpeedBoard {
    speeds: HashMap<String, u64>,
}

impl SpeedBoard {
    pub fn apply(&mut self, ev: &EngineEvent) {
        match ev {
            EngineEvent::Progress(p) => {
                self.speeds.insert(p.id.clone(), p.speed_bps);
            }
            EngineEvent::State { id, state, .. } if !state.is_active() => {
                self.speeds.remove(id);
            }
            EngineEvent::Removed { id } => {
                self.speeds.remove(id);
            }
            _ => {}
        }
    }

    pub fn total(&self) -> u64 {
        self.speeds.values().sum()
    }

    pub fn clear(&mut self) {
        self.speeds.clear();
    }
}

/// Spawns a task that feeds every manager event into the board; on Lagged it clears the board.
pub fn track(m: &ManagerHandle) -> Arc<Mutex<SpeedBoard>> {
    let board = Arc::new(Mutex::new(SpeedBoard::default()));
    let mut rx = m.subscribe();
    let b = board.clone();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(ev) => b.lock().unwrap_or_else(|e| e.into_inner()).apply(&ev),
                Err(RecvError::Lagged(_)) => b.lock().unwrap_or_else(|e| e.into_inner()).clear(),
                Err(RecvError::Closed) => break,
            }
        }
    });
    board
}

#[cfg(test)]
mod tests {
    use super::*;
    use odm_core::DownloadState;
    use odm_engine::ProgressSnapshot;

    fn progress(id: &str, speed: u64) -> EngineEvent {
        EngineEvent::Progress(ProgressSnapshot {
            id: id.into(),
            bytes: 0,
            total: None,
            speed_bps: speed,
            eta_s: None,
            chunks: vec![],
            segments: None,
        })
    }

    #[test]
    fn speed_board_tracks_active_downloads() {
        let mut b = SpeedBoard::default();
        b.apply(&progress("a", 100));
        b.apply(&progress("b", 50));
        assert_eq!(b.total(), 150);
        b.apply(&EngineEvent::State {
            id: "a".into(),
            state: DownloadState::Paused,
            error_code: None,
            error_message: None,
        });
        assert_eq!(b.total(), 50);
        b.apply(&EngineEvent::Removed { id: "b".into() });
        assert_eq!(b.total(), 0);
    }
}
