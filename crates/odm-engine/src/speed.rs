use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub struct SpeedMeter {
    window: Duration,
    samples: VecDeque<(Instant, u64)>,
}

impl SpeedMeter {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            samples: VecDeque::new(),
        }
    }

    pub fn record(&mut self, now: Instant, total: u64) {
        self.samples.push_back((now, total));
        while self.samples.len() > 1 && now.duration_since(self.samples[0].0) > self.window {
            self.samples.pop_front();
        }
    }

    pub fn bps(&self) -> u64 {
        let (Some(&(t0, b0)), Some(&(t1, b1))) = (self.samples.front(), self.samples.back()) else {
            return 0;
        };
        let dt = t1.duration_since(t0).as_secs_f64();
        if self.samples.len() < 2 || dt <= 0.0 {
            return 0;
        }
        ((b1.saturating_sub(b0)) as f64 / dt) as u64
    }
}

pub fn eta(total: Option<u64>, bytes: u64, bps: u64) -> Option<u64> {
    let total = total?;
    (bps > 0).then(|| total.saturating_sub(bytes) / bps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn moving_average_over_window() {
        let t0 = Instant::now();
        let mut m = SpeedMeter::new(Duration::from_secs(5));
        assert_eq!(m.bps(), 0);
        m.record(t0, 0);
        m.record(t0 + Duration::from_secs(1), 1_000);
        assert_eq!(m.bps(), 1_000);
        m.record(t0 + Duration::from_secs(2), 3_000);
        assert_eq!(m.bps(), 1_500);
        // Samples older than 5 s drop out: window now (4s..8s]
        m.record(t0 + Duration::from_secs(8), 3_000);
        assert_eq!(m.bps(), 0);
    }

    #[test]
    fn eta_math() {
        assert_eq!(eta(Some(10_000), 4_000, 1_000), Some(6));
        assert_eq!(eta(None, 4_000, 1_000), None);
        assert_eq!(eta(Some(10_000), 4_000, 0), None);
    }
}
