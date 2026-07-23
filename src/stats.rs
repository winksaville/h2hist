//! Midpoint-weighted summary statistics over bucket streams.
//!
//! - `Stats` carries count, mean, and **variance** — not
//!   stdev, because `core` has no `sqrt`; `stdev()` is offered
//!   under `std` (a `libm` feature stays open). Variance is
//!   monotonic in stdev, so comparisons work either way.
//! - Two passes: mean first, then `(mid - mean)²` mass — the
//!   textbook form. The one-pass `sumsq/n - mean²` shortcut
//!   cancels catastrophically when the mean is large relative
//!   to the spread, which is exactly the latency case.
//! - A rank window `(lo, hi]` scopes the stats to part of the
//!   run — `(0, total]` is the overall row, `(0, rank(n2)]`
//!   the tail-trimmed row, `(rank(p10), rank(p90)]` a core
//!   window. Windows split buckets by exact rank span (the
//!   [`RankSplit`](crate::RankSplit) convention).

use crate::analysis::Bucket;

/// Midpoint-weighted count, mean, and variance of the ranks
/// inside a window.
///
/// - `count` — ranks covered; `mean` / `variance` are 0.0
///   when it is 0 (an empty window has no moments).
/// - `variance` is the population variance of the
///   bucket-midpoint values, weighted by each bucket's ranks
///   in the window — accurate to the bucketing's relative
///   error, like the original HdrHistogram's derived stats.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stats {
    /// Ranks covered by the window.
    pub count: u64,
    /// Midpoint-weighted mean.
    pub mean: f64,
    /// Midpoint-weighted population variance.
    pub variance: f64,
}

impl Stats {
    /// Stats over every recorded rank. `buckets` is called
    /// twice (once per pass), so pass a cheap constructor —
    /// e.g. `|| hist.buckets()`.
    pub fn from_buckets<Iter, Make>(buckets: Make) -> Stats
    where
        Make: Fn() -> Iter,
        Iter: Iterator<Item = Bucket>,
    {
        Stats::from_window(buckets, 0, u64::MAX)
    }

    /// Stats over the ranks in `(lo, hi]` (1-based, so
    /// `(0, total]` covers everything). `buckets` is called
    /// twice (once per pass).
    pub fn from_window<Iter, Make>(buckets: Make, lo: u64, hi: u64) -> Stats
    where
        Make: Fn() -> Iter,
        Iter: Iterator<Item = Bucket>,
    {
        // Pass 1: windowed count and midpoint-weighted sum.
        let mut count = 0u64;
        let mut sum = 0f64;
        for bucket in buckets() {
            let (overlap, mid) = window_overlap(&bucket, lo, hi);
            if overlap > 0 {
                count += overlap;
                sum += overlap as f64 * mid;
            }
        }
        if count == 0 {
            return Stats::default();
        }
        let mean = sum / count as f64;

        // Pass 2: centered second moment — no cancellation.
        let mut var_sum = 0f64;
        for bucket in buckets() {
            let (overlap, mid) = window_overlap(&bucket, lo, hi);
            if overlap > 0 {
                let diff = mid - mean;
                var_sum += overlap as f64 * diff * diff;
            }
        }
        Stats {
            count,
            mean,
            variance: var_sum / count as f64,
        }
    }

    /// Standard deviation, `sqrt(variance)`. `std`-only:
    /// `core` has no `sqrt`; `no_std` consumers compare
    /// variances or enable a future `libm` feature.
    #[cfg(feature = "std")]
    pub fn stdev(&self) -> f64 {
        self.variance.sqrt()
    }
}

/// A bucket's rank overlap with the window `(lo, hi]` and its
/// value midpoint.
fn window_overlap(bucket: &Bucket, lo: u64, hi: u64) -> (u64, f64) {
    if bucket.count == 0 {
        return (0, 0.0);
    }
    // The bucket's ranks are (cumulative - count, cumulative].
    let start = bucket.cumulative - bucket.count;
    let from = if start > lo { start } else { lo };
    let to = if bucket.cumulative < hi {
        bucket.cumulative
    } else {
        hi
    };
    if to <= from {
        return (0, 0.0);
    }
    let mid = (bucket.low as f64 + bucket.high as f64) / 2.0;
    (to - from, mid)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // OK: tests panic on setup failure by design
mod tests {
    use super::*;
    use crate::{Config, Histogram};

    /// Build a config at compile time; invalid powers are a
    /// test-authoring error, so they fail the build rather
    /// than the run.
    const fn cfg(grouping_power: u8, max_value_power: u8) -> Config {
        match Config::new(grouping_power, max_value_power) {
            Ok(config) => config,
            Err(_) => panic!("invalid test config"),
        }
    }

    /// 1..=5 once each in the exact region: mean 3,
    /// population variance 2.
    #[test]
    fn exact_small_set() {
        const CFG: Config = cfg(3, 10);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        for value in 1..=5u64 {
            hist.record(value);
        }
        let stats = Stats::from_buckets(|| hist.buckets());
        assert_eq!(stats.count, 5);
        assert!((stats.mean - 3.0).abs() < 1e-12);
        assert!((stats.variance - 2.0).abs() < 1e-12);
        assert!((stats.stdev() - 2f64.sqrt()).abs() < 1e-12);
    }

    /// A window drops the ranks outside it: 1..=100 recorded,
    /// window (0, 99] is the order statistics of 1..=99.
    #[test]
    fn window_trims_ranks() {
        const CFG: Config = cfg(7, 20);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        for value in 1..=100u64 {
            hist.record(value);
        }
        let stats = Stats::from_window(|| hist.buckets(), 0, 99);
        assert_eq!(stats.count, 99);
        assert!((stats.mean - 50.0).abs() < 1e-12);
        // Population variance of 1..=99: (99² - 1) / 12.
        let expected = (99.0 * 99.0 - 1.0) / 12.0;
        assert!((stats.variance - expected).abs() < 1e-9);

        // A core window (p10..p90 shape): ranks 11..=90.
        let core = Stats::from_window(|| hist.buckets(), 10, 90);
        assert_eq!(core.count, 80);
        assert!((core.mean - 50.5).abs() < 1e-12);
    }

    /// A window can split one bucket's ranks.
    #[test]
    fn window_splits_a_bucket() {
        const CFG: Config = cfg(2, 8);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        hist.record_n(5, 10);
        let stats = Stats::from_window(|| hist.buckets(), 0, 4);
        assert_eq!(stats.count, 4);
        assert!((stats.mean - 5.0).abs() < 1e-12);
        assert_eq!(stats.variance, 0.0);
    }

    /// The two-pass form survives a large mean with a tiny
    /// spread, where `sumsq/n - mean²` loses the answer to
    /// cancellation (~2e18-scale squares against an f64's
    /// ~1e-16 relative precision).
    #[test]
    fn two_pass_beats_cancellation() {
        let base = 1_000_000_000u64;
        let make = || {
            [
                Bucket {
                    low: base,
                    high: base,
                    count: 1,
                    cumulative: 1,
                },
                Bucket {
                    low: base + 2,
                    high: base + 2,
                    count: 1,
                    cumulative: 2,
                },
            ]
            .into_iter()
        };
        let stats = Stats::from_buckets(make);
        assert_eq!(stats.count, 2);
        assert!((stats.mean - (base as f64 + 1.0)).abs() < 1e-6);
        // Exact answer is 1.0; the one-pass shortcut loses it
        // entirely — at ~1e18 the f64 ulp of the squares (128)
        // dwarfs the answer, and the difference collapses to 0.
        assert!((stats.variance - 1.0).abs() < 1e-9);
        let naive = {
            let mean = base as f64 + 1.0;
            let sumsq = (base as f64).powi(2) + (base as f64 + 2.0).powi(2);
            sumsq / 2.0 - mean * mean
        };
        assert!((naive - 1.0).abs() > 0.5, "naive={naive}");
    }

    /// Empty histogram and empty window yield zeroed stats.
    #[test]
    fn empty_is_zeroed() {
        const CFG: Config = cfg(2, 8);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        let empty = Stats::from_buckets(|| hist.buckets());
        assert_eq!(empty, Stats::default());

        hist.record(3);
        let outside = Stats::from_window(|| hist.buckets(), 5, 10);
        assert_eq!(outside, Stats::default());
    }
}
