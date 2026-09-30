//! Token bucket that allows debt, so a request larger than the bucket
//! waits proportionally instead of spinning forever.
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Bucket {
    rate: u64,
    tokens: f64,
    last: Instant,
}

pub struct SpeedLimiter {
    inner: Mutex<Bucket>,
}

impl SpeedLimiter {
    pub fn new(rate_bps: u64) -> Self {
        Self {
            inner: Mutex::new(Bucket {
                rate: rate_bps,
                tokens: rate_bps as f64,
                last: Instant::now(),
            }),
        }
    }

    pub fn rate(&self) -> u64 {
        self.inner.lock().unwrap().rate
    }

    pub fn set_rate(&self, bps: u64) {
        let mut b = self.inner.lock().unwrap();
        b.rate = bps;
        b.tokens = bps as f64;
        b.last = Instant::now();
    }

    pub async fn acquire(&self, n: usize) {
        let wait = {
            let mut b = self.inner.lock().unwrap();
            if b.rate == 0 {
                return;
            }
            let now = Instant::now();
            let rate = b.rate as f64;
            b.tokens = (b.tokens + now.duration_since(b.last).as_secs_f64() * rate).min(rate);
            b.last = now;
            b.tokens -= n as f64;
            if b.tokens < 0.0 {
                Duration::from_secs_f64(-b.tokens / rate)
            } else {
                Duration::ZERO
            }
        }; // lock released before sleeping
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}
