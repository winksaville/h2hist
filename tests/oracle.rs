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

#[path = "../dev/mod.rs"]
mod dev;

use dev::consts::{BUCKETS, CFG, GROUPING_POWER, HDR_SIGFIG, MAX_VALUE_POWER, SEED};
use dev::rng::SplitMix64;
use dev::stream::HeavyTailed;

/// Configs swept for exact counts parity: a tiny one, a
/// mid-size one, the shared dev config, and a wide one.
const PARITY_CONFIGS: [(u8, u8); 4] =
    [(2, 10), (4, 20), (GROUPING_POWER, MAX_VALUE_POWER), (7, 40)];

/// Seed for the counts-parity sweep, kept distinct from
/// [`SEED`] so the sweep and the quantile test exercise
/// different streams; per-config bits are mixed in below.
const PARITY_SEED: u64 = 0xDEAD_BEEF;

/// Samples per config in the counts-parity sweep.
const PARITY_SAMPLES: usize = 50_000;

/// Samples in the quantile-parity stream.
const QUANTILE_SAMPLES: usize = 200_000;

/// Exact counts parity with the h2 oracle across configs and
/// a uniform-random stream (over-max values excluded — the
/// oracle rejects them while we clamp).
#[test]
fn h2_counts_parity_uniform() {
    for (grouping_power, max_value_power) in PARITY_CONFIGS {
        let config = Config::new(grouping_power, max_value_power).unwrap();
        let mut ours_storage = vec![0u64; config.total_buckets()];
        let mut ours = Histogram::new(config, &mut ours_storage).unwrap();
        let mut oracle = histogram::Histogram::new(grouping_power, max_value_power).unwrap();

        let mut rng =
            SplitMix64(PARITY_SEED ^ ((grouping_power as u64) << 8) ^ max_value_power as u64);
        for _ in 0..PARITY_SAMPLES {
            let value = rng.next() & config.max_value();
            ours.record(value);
            oracle.increment(value).unwrap();
        }

        let oracle_counts = oracle.as_slice();
        assert_eq!(
            config.total_buckets(),
            oracle_counts.len(),
            "bucket count differs from oracle g={grouping_power} n={max_value_power}"
        );
        for (index, &oracle_count) in oracle_counts.iter().enumerate() {
            assert_eq!(
                ours.count_at(index),
                Some(oracle_count),
                "bucket {index} differs g={grouping_power} n={max_value_power}"
            );
        }
    }
}

/// Boundary values (power-of-two edges) land in the same
/// buckets as the h2 oracle.
#[test]
fn h2_counts_parity_boundaries() {
    let (grouping_power, max_value_power) = (5u8, 24u8);
    let config = Config::new(grouping_power, max_value_power).unwrap();
    let mut ours_storage = vec![0u64; config.total_buckets()];
    let mut ours = Histogram::new(config, &mut ours_storage).unwrap();
    let mut oracle = histogram::Histogram::new(grouping_power, max_value_power).unwrap();

    let mut push = |value: u64| {
        ours.record(value);
        oracle.increment(value).unwrap();
    };
    push(0);
    push(config.max_value());
    for exp in 0..max_value_power as u32 {
        let power_of_two = 1u64 << exp;
        push(power_of_two);
        push(power_of_two - 1);
        if power_of_two < config.max_value() {
            push(power_of_two + 1);
        }
    }

    for (index, &oracle_count) in oracle.as_slice().iter().enumerate() {
        assert_eq!(ours.count_at(index), Some(oracle_count), "bucket {index}");
    }
}

/// Quantile parity with hdrhistogram on a heavy-tailed
/// stream: p50..p99.9999 agree within the combined relative
/// tolerance of both bucketings (2^-g ours, ~1% at 2 sigfig
/// theirs), plus 1 for integer slack.
#[test]
fn hdr_quantile_parity() {
    let mut ours_storage = [0u32; BUCKETS];
    let mut ours = Histogram::new(CFG, &mut ours_storage).unwrap();
    let mut hdr =
        hdrhistogram::Histogram::<u64>::new_with_bounds(1, CFG.max_value(), HDR_SIGFIG).unwrap();

    for value in HeavyTailed::new(SEED, CFG.max_value()).take(QUANTILE_SAMPLES) {
        ours.record(value);
        hdr.record(value).unwrap();
    }

    assert_eq!(ours.total(), hdr.len());
    // Our quantization bound (2^-g) plus theirs (10^-sigfig).
    let tol = 2f64.powi(-(GROUPING_POWER as i32)) + 10f64.powi(-(HDR_SIGFIG as i32));
    for fraction in [0.0, 0.25, 0.5, 0.9, 0.99, 0.999, 0.9999, 0.999_999, 1.0] {
        let ours_val = ours.quantile(fraction).unwrap() as f64;
        let hdr_val = hdr.value_at_quantile(fraction) as f64;
        let rel = (ours_val - hdr_val).abs() / ours_val.max(hdr_val).max(1.0);
        assert!(
            rel <= tol + 1.0 / ours_val.max(hdr_val).max(1.0),
            "q={fraction}: ours={ours_val} hdr={hdr_val} rel={rel:.4} tol={tol:.4}"
        );
    }
}
