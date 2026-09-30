use odm_engine::SpeedLimiter;
use std::time::{Duration, Instant};

#[tokio::test]
async fn unlimited_is_instant() {
    let l = SpeedLimiter::new(0);
    let t = Instant::now();
    l.acquire(100_000_000).await;
    assert!(t.elapsed() < Duration::from_millis(50));
}

#[tokio::test]
async fn request_larger_than_bucket_completes() {
    // Regression: the old limiter looped forever here.
    let l = SpeedLimiter::new(10_000);
    let t = Instant::now();
    l.acquire(15_000).await; // 10k burst, 5k debt -> ~0.5 s
    let e = t.elapsed();
    assert!(
        e >= Duration::from_millis(350) && e < Duration::from_secs(2),
        "{e:?}"
    );
}

#[tokio::test]
async fn tiny_limit_below_buffer_size_does_not_hang() {
    let l = SpeedLimiter::new(4_096);
    let t = Instant::now();
    l.acquire(8_192).await; // 4k burst, 4k debt -> ~1 s
    assert!(t.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn sustained_rate_is_respected() {
    let l = SpeedLimiter::new(20_000);
    let t = Instant::now();
    for _ in 0..10 {
        l.acquire(5_000).await; // 50k total, 20k burst -> ~1.5 s
    }
    let e = t.elapsed();
    assert!(
        e >= Duration::from_millis(1_200) && e < Duration::from_millis(2_500),
        "{e:?}"
    );
}

#[tokio::test]
async fn switching_to_unlimited_takes_effect() {
    let l = SpeedLimiter::new(1_000);
    l.set_rate(0);
    assert_eq!(l.rate(), 0);
    let t = Instant::now();
    l.acquire(1_000_000).await;
    assert!(t.elapsed() < Duration::from_millis(50));
}
