//! Oracle parity suite.
//!
//! - vs iopsystems `histogram` (same h2 scheme): identical
//!   streams must produce **identical counts arrays** — the
//!   strongest possible check on the index math.
//! - vs `hdrhistogram` (the reference implementation):
//!   quantiles must agree within the combined
//!   equivalent-value tolerance of the two bucketings.
//! - Streams come from a seeded splitmix64: deterministic,
//!   no rand dependency.

#![allow(clippy::unwrap_used)] // OK: tests panic on setup failure by design

use h2hist::{Config, Histogram};

/// Deterministic 64-bit PRNG (splitmix64).
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

/// Exact counts parity with the h2 oracle across configs and
/// a uniform-random stream (over-max values excluded — the
/// oracle rejects them while we clamp).
#[test]
fn h2_counts_parity_uniform() {
    for (g, n) in [(2u8, 10u8), (4, 20), (7, 30), (7, 40)] {
        let cfg = Config::new(g, n).unwrap();
        let mut ours_storage = vec![0u64; cfg.total_buckets()];
        let mut ours = Histogram::new(cfg, &mut ours_storage).unwrap();
        let mut oracle = histogram::Histogram::new(g, n).unwrap();

        let mut rng = SplitMix64(0xDEAD_BEEF ^ ((g as u64) << 8) ^ n as u64);
        for _ in 0..50_000 {
            let v = rng.next() & cfg.max_value();
            ours.record(v);
            oracle.increment(v).unwrap();
        }

        let oracle_counts = oracle.as_slice();
        assert_eq!(
            cfg.total_buckets(),
            oracle_counts.len(),
            "bucket count differs from oracle g={g} n={n}"
        );
        for (i, &oc) in oracle_counts.iter().enumerate() {
            assert_eq!(ours.count_at(i), Some(oc), "bucket {i} differs g={g} n={n}");
        }
    }
}

/// Boundary values (power-of-two edges) land in the same
/// buckets as the h2 oracle.
#[test]
fn h2_counts_parity_boundaries() {
    let (g, n) = (5u8, 24u8);
    let cfg = Config::new(g, n).unwrap();
    let mut ours_storage = vec![0u64; cfg.total_buckets()];
    let mut ours = Histogram::new(cfg, &mut ours_storage).unwrap();
    let mut oracle = histogram::Histogram::new(g, n).unwrap();

    let mut push = |v: u64| {
        ours.record(v);
        oracle.increment(v).unwrap();
    };
    push(0);
    push(cfg.max_value());
    for k in 0..n as u32 {
        let p = 1u64 << k;
        push(p);
        push(p - 1);
        if p < cfg.max_value() {
            push(p + 1);
        }
    }

    for (i, &oc) in oracle.as_slice().iter().enumerate() {
        assert_eq!(ours.count_at(i), Some(oc), "bucket {i}");
    }
}

/// Quantile parity with hdrhistogram on a heavy-tailed
/// stream: p50..p99.9999 agree within the combined relative
/// tolerance of both bucketings (2^-g ours, ~1% at 2 sigfig
/// theirs), plus 1 for integer slack.
#[test]
fn hdr_quantile_parity() {
    let (g, n) = (7u8, 30u8);
    let cfg = Config::new(g, n).unwrap();
    let mut ours_storage = vec![0u32; cfg.total_buckets()];
    let mut ours = Histogram::new(cfg, &mut ours_storage).unwrap();
    let mut hdr = hdrhistogram::Histogram::<u64>::new_with_bounds(1, cfg.max_value(), 2).unwrap();

    // Heavy-tailed synthetic latency: ~100-tick body with a
    // 1-in-1000 tail stretching multiplicatively.
    let mut rng = SplitMix64(42);
    for _ in 0..200_000 {
        let base = 50 + (rng.next() % 100);
        let v = match rng.next() % 1000 {
            0 => base * (1 + rng.next() % 10_000),
            1..=9 => base * (1 + rng.next() % 100),
            _ => base,
        }
        .clamp(1, cfg.max_value());
        ours.record(v);
        hdr.record(v).unwrap();
    }

    assert_eq!(ours.total(), hdr.len());
    let tol = 2f64.powi(-(g as i32)) + 0.01;
    for q in [0.0, 0.25, 0.5, 0.9, 0.99, 0.999, 0.9999, 0.999_999, 1.0] {
        let a = ours.quantile(q).unwrap() as f64;
        let b = hdr.value_at_quantile(q) as f64;
        let rel = (a - b).abs() / a.max(b).max(1.0);
        assert!(
            rel <= tol + 1.0 / a.max(b).max(1.0),
            "q={q}: ours={a} hdr={b} rel={rel:.4} tol={tol:.4}"
        );
    }
}
