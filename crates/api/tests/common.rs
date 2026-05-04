use rand::Rng;
use std::sync::atomic::{AtomicI64, Ordering};

static NEXT_UNIQUE_I64: AtomicI64 = AtomicI64::new(0);

pub fn unique_i64() -> i64 {
    if NEXT_UNIQUE_I64.load(Ordering::Relaxed) == 0 {
        // Generate GitHub-ID-like positive values and keep them unique within this test process.
        let seed = rand::thread_rng().gen_range(1..=(i64::MAX / 2));
        let _ = NEXT_UNIQUE_I64.compare_exchange(0, seed, Ordering::Relaxed, Ordering::Relaxed);
    }

    NEXT_UNIQUE_I64.fetch_add(1, Ordering::Relaxed)
}
