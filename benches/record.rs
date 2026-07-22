//! Record-path cost bench (hand-rolled, `harness = false`).
//!
//! - Times ns/record over a pre-generated heavy-tailed value
//!   stream for: a raw streaming store (the ArrayRecorder
//!   pattern), our u32 histogram, the same plus inline
//!   min/max/sum tracking (the "hot-path extras" candidates),
//!   and `hdrhistogram` — best of `REPS` passes each.
//! - Purpose: absolute record cost vs alternatives, and the
//!   marginal cost of the extras — the data for the parked
//!   hot-path-extras decision.

use std::hint::black_box;
use std::time::Instant;

use histogram_no_std::{Config, Error, Histogram};

/// Samples per pass.
const LEN: usize = 8_000_000;
/// Timed passes per variant; best is reported.
const REPS: usize = 3;

/// Deterministic 64-bit PRNG (splitmix64); same shape as the
/// oracle suite.
struct SplitMix64(u64);

impl SplitMix64 {
    /// Next raw u64.
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Heavy-tailed synthetic stream, identical shape to h2demo.
fn make_values(max_value: u64) -> Vec<u64> {
    let mut rng = SplitMix64(42);
    (0..LEN)
        .map(|_| {
            let base = 50 + (rng.next() % 100);
            match rng.next() % 1000 {
                0 => base * (1 + rng.next() % 10_000),
                1..=9 => base * (1 + rng.next() % 100),
                _ => base,
            }
            .clamp(1, max_value)
        })
        .collect()
}

/// Best-of-REPS wall time of `f`, reported as ns per value.
fn bench(label: &str, mut f: impl FnMut()) {
    let mut best = f64::MAX;
    for _ in 0..REPS {
        let t0 = Instant::now();
        f();
        let ns = t0.elapsed().as_nanos() as f64;
        let per = ns / LEN as f64;
        if per < best {
            best = per;
        }
    }
    println!("{label:<22} {best:>8.3} ns/record");
}

/// Run all variants and print the comparison table.
fn main() -> Result<(), Error> {
    let cfg = Config::new(7, 30)?;
    let values = make_values(cfg.max_value());

    println!(
        "record bench: g=7 n=30 buckets={} len={} reps={} (best)",
        cfg.total_buckets(),
        LEN,
        REPS
    );

    let mut store = vec![0u64; LEN];
    bench("raw streaming store", || {
        for (slot, &v) in store.iter_mut().zip(values.iter()) {
            *slot = v;
        }
        black_box(&store);
    });

    let mut counts = vec![0u32; cfg.total_buckets()];
    let mut h = Histogram::new(cfg, &mut counts)?;
    bench("histogram u32", || {
        for &v in &values {
            h.record(v);
        }
        black_box(h.total());
    });

    let mut counts2 = vec![0u32; cfg.total_buckets()];
    let mut h2 = Histogram::new(cfg, &mut counts2)?;
    bench("histogram u32 + extras", || {
        let (mut mn, mut mx, mut sum) = (u64::MAX, 0u64, 0u64);
        for &v in &values {
            h2.record(v);
            mn = mn.min(v);
            mx = mx.max(v);
            sum = sum.wrapping_add(v);
        }
        black_box((mn, mx, sum, h2.total()));
    });

    match hdrhistogram::Histogram::<u64>::new_with_bounds(1, cfg.max_value(), 2) {
        Ok(mut hdr) => {
            bench("hdrhistogram 2sf", || {
                for &v in &values {
                    // Values are pre-clamped, record cannot fail.
                    let _ = hdr.record(v);
                }
                black_box(hdr.len());
            });
        }
        Err(e) => println!("hdrhistogram setup failed: {e:?}"),
    }

    Ok(())
}
