use odm_store::ChunkRecord;
use std::sync::{Arc, Mutex};

use crate::events::{ChunkView, ChunkViewState};

pub const MIN_CHUNK: u64 = 1 << 20;
pub const MIN_SPLIT_REMAINING: u64 = 2 << 20;

pub fn plan_chunks(
    size: Option<u64>,
    supports_range: bool,
    max_connections: u32,
) -> Vec<ChunkRecord> {
    match size {
        Some(0) => Vec::new(),
        Some(size) if supports_range => {
            let n = (size / MIN_CHUNK).clamp(1, max_connections.max(1) as u64);
            let base = size / n;
            (0..n)
                .map(|i| {
                    let start = i * base;
                    let end = if i == n - 1 {
                        size - 1
                    } else {
                        start + base - 1
                    };
                    ChunkRecord {
                        idx: i as u32,
                        start,
                        end: Some(end),
                        written: 0,
                    }
                })
                .collect()
        }
        Some(size) => vec![ChunkRecord {
            idx: 0,
            start: 0,
            end: Some(size - 1),
            written: 0,
        }],
        None => vec![ChunkRecord {
            idx: 0,
            start: 0,
            end: None,
            written: 0,
        }],
    }
}

#[derive(Debug, Clone)]
pub struct ChunkShared {
    pub idx: u32,
    pub start: u64,
    pub end: Option<u64>,
    /// Bytes reserved by a worker (>= written).
    pub claimed: u64,
    /// Bytes written AND flushed-eligible; the only value persisted.
    pub written: u64,
    /// UI-only state, set by the transfer loop; never persisted.
    pub state: ChunkViewState,
}

impl ChunkShared {
    pub fn remaining(&self) -> Option<u64> {
        self.end
            .map(|e| (e + 1).saturating_sub(self.start + self.claimed))
    }

    pub fn is_done(&self) -> bool {
        matches!(self.end, Some(e) if self.start + self.written > e)
    }

    /// Reserve up to `want` bytes; returns (file offset, allowed bytes).
    pub fn claim(&mut self, want: usize) -> (u64, usize) {
        let offset = self.start + self.claimed;
        let allowed = match self.remaining() {
            Some(r) => (want as u64).min(r) as usize,
            None => want,
        };
        self.claimed += allowed as u64;
        (offset, allowed)
    }
}

pub type ChunkRef = Arc<Mutex<ChunkShared>>;

pub struct ChunkTable {
    chunks: Vec<ChunkRef>,
}

impl ChunkTable {
    pub fn from_records(recs: &[ChunkRecord]) -> Self {
        let chunks = recs
            .iter()
            .map(|r| {
                let mut c = ChunkShared {
                    idx: r.idx,
                    start: r.start,
                    end: r.end,
                    claimed: r.written,
                    written: r.written,
                    state: ChunkViewState::Pending,
                };
                if c.is_done() {
                    c.state = ChunkViewState::Done; // completed in an earlier session
                }
                Arc::new(Mutex::new(c))
            })
            .collect();
        Self { chunks }
    }

    pub fn refs(&self) -> Vec<ChunkRef> {
        self.chunks.clone()
    }

    pub fn pending(&self) -> Vec<ChunkRef> {
        self.chunks
            .iter()
            .filter(|c| !c.lock().unwrap().is_done())
            .cloned()
            .collect()
    }

    pub fn split_largest(&mut self) -> Option<ChunkRef> {
        let next_idx = self
            .chunks
            .iter()
            .map(|c| c.lock().unwrap().idx)
            .max()
            .map_or(0, |m| m + 1);
        let victim = self
            .chunks
            .iter()
            .filter_map(|c| c.lock().unwrap().remaining().map(|r| (r, c.clone())))
            .filter(|(r, _)| *r > MIN_SPLIT_REMAINING)
            .max_by_key(|(r, _)| *r)?
            .1;
        let mut v = victim.lock().unwrap();
        let remaining = v.remaining()?; // re-check under the lock
        if remaining <= MIN_SPLIT_REMAINING {
            return None;
        }
        let cur = v.start + v.claimed;
        let mid = cur + remaining / 2;
        let new = ChunkShared {
            idx: next_idx,
            start: mid,
            end: v.end,
            claimed: 0,
            written: 0,
            state: ChunkViewState::Pending,
        };
        v.end = Some(mid - 1);
        drop(v);
        let r = Arc::new(Mutex::new(new));
        self.chunks.push(r.clone());
        Some(r)
    }

    pub fn snapshot(&self) -> Vec<ChunkRecord> {
        let mut v: Vec<ChunkRecord> = self
            .chunks
            .iter()
            .map(|c| {
                let c = c.lock().unwrap();
                ChunkRecord {
                    idx: c.idx,
                    start: c.start,
                    end: c.end,
                    written: c.written,
                }
            })
            .collect();
        v.sort_by_key(|c| c.start);
        v
    }

    /// UI view of every chunk, ordered by start offset.
    pub fn views(&self) -> Vec<ChunkView> {
        let mut v: Vec<ChunkView> = self
            .chunks
            .iter()
            .map(|c| {
                let c = c.lock().unwrap();
                ChunkView {
                    idx: c.idx,
                    start: c.start,
                    end: c.end,
                    written: c.written,
                    state: c.state,
                }
            })
            .collect();
        v.sort_by_key(|c| c.start);
        v
    }

    pub fn progress(&self) -> Vec<(u32, u64)> {
        self.chunks
            .iter()
            .map(|c| {
                let c = c.lock().unwrap();
                (c.idx, c.written)
            })
            .collect()
    }

    pub fn total_written(&self) -> u64 {
        self.chunks.iter().map(|c| c.lock().unwrap().written).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const MB: u64 = 1 << 20;

    #[test]
    fn plan_unknown_size_is_single_open_chunk() {
        let p = plan_chunks(None, true, 8);
        assert_eq!(
            p,
            vec![ChunkRecord {
                idx: 0,
                start: 0,
                end: None,
                written: 0
            }]
        );
    }

    #[test]
    fn plan_no_range_is_single_chunk() {
        let p = plan_chunks(Some(100 * MB), false, 8);
        assert_eq!(
            p,
            vec![ChunkRecord {
                idx: 0,
                start: 0,
                end: Some(100 * MB - 1),
                written: 0
            }]
        );
    }

    #[test]
    fn plan_zero_size_is_empty() {
        assert!(plan_chunks(Some(0), true, 8).is_empty());
    }

    #[test]
    fn plan_respects_min_chunk_and_max_conn() {
        assert_eq!(plan_chunks(Some(3 * MB), true, 8).len(), 3);
        assert_eq!(plan_chunks(Some(500 * MB), true, 8).len(), 8);
        assert_eq!(plan_chunks(Some(10), true, 8).len(), 1);
    }

    #[test]
    fn plan_covers_every_byte_exactly_once() {
        let size = 10 * MB + 12345;
        let p = plan_chunks(Some(size), true, 7);
        assert_eq!(p[0].start, 0);
        for w in p.windows(2) {
            assert_eq!(w[0].end.unwrap() + 1, w[1].start);
        }
        assert_eq!(p.last().unwrap().end, Some(size - 1));
    }

    #[test]
    fn claim_stops_at_end() {
        let mut c = ChunkShared {
            idx: 0,
            start: 100,
            end: Some(109),
            claimed: 0,
            written: 0,
            state: ChunkViewState::Pending,
        };
        assert_eq!(c.claim(4), (100, 4));
        assert_eq!(c.claim(100), (104, 6));
        assert_eq!(c.claim(8), (110, 0));
        assert_eq!(c.remaining(), Some(0));
    }

    #[test]
    fn claim_open_ended_is_unbounded() {
        let mut c = ChunkShared {
            idx: 0,
            start: 0,
            end: None,
            claimed: 0,
            written: 0,
            state: ChunkViewState::Pending,
        };
        assert_eq!(c.claim(8192), (0, 8192));
        assert_eq!(c.remaining(), None);
        assert!(!c.is_done());
    }

    #[test]
    fn split_takes_upper_half_of_remaining() {
        let recs = vec![
            ChunkRecord {
                idx: 0,
                start: 0,
                end: Some(10 * MB - 1),
                written: 0,
            },
            ChunkRecord {
                idx: 1,
                start: 10 * MB,
                end: Some(11 * MB - 1),
                written: 0,
            },
        ];
        let mut t = ChunkTable::from_records(&recs);
        {
            let r = t.refs();
            r[0].lock().unwrap().claim((2 * MB) as usize); // worker advanced 2 MB
        }
        let new = t.split_largest().expect("split");
        let n = new.lock().unwrap();
        // remaining of chunk 0 is [2MB, 10MB-1] = 8MB -> mid at 6MB
        assert_eq!((n.idx, n.start, n.end), (2, 6 * MB, Some(10 * MB - 1)));
        assert_eq!(t.refs()[0].lock().unwrap().end, Some(6 * MB - 1));
    }

    #[test]
    fn split_refuses_small_or_open_chunks() {
        // Remaining exactly 2 MiB -> not strictly greater than MIN_SPLIT_REMAINING -> no split.
        let mut t = ChunkTable::from_records(&[ChunkRecord {
            idx: 0,
            start: 0,
            end: Some(2 * MB - 1),
            written: 0,
        }]);
        assert!(t.split_largest().is_none());
        let mut t = ChunkTable::from_records(&[ChunkRecord {
            idx: 0,
            start: 0,
            end: None,
            written: 0,
        }]);
        assert!(t.split_largest().is_none());
    }

    #[test]
    fn snapshot_uses_written_not_claimed() {
        let mut t = ChunkTable::from_records(&[ChunkRecord {
            idx: 0,
            start: 0,
            end: Some(99),
            written: 10,
        }]);
        {
            let r = t.refs();
            let mut c = r[0].lock().unwrap();
            assert_eq!(c.claim(50), (10, 50)); // resumes at written
            c.written += 20; // only 20 of the 50 claimed bytes flushed
        }
        assert_eq!(t.snapshot()[0].written, 30);
        assert_eq!(t.total_written(), 30);
        assert!(t.split_largest().is_none());
    }
}
