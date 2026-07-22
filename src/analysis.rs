//! Off-hot-path analysis: bucket iteration, quantiles, merge.
//!
//! - Shared by the borrowed and owned histogram types; all
//!   O(buckets), `no_std`, integer math except the quantile
//!   input itself.
//! - The iterator carries **cumulative** counts so band
//!   tables (quantile fences, trimmed stats) fall out of one
//!   pass — the readout requirement in ARCHITECTURE.md.

use crate::config::{Config, Error};
use crate::counter::Counter;

/// One bucket as seen by the iterator.
///
/// - `low`/`high` — inclusive value range.
/// - `count` — occurrences recorded in this bucket.
/// - `cumulative` — occurrences in this bucket and every
///   bucket below it (saturating).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bucket {
    /// Inclusive low end of the bucket's value range.
    pub low: u64,
    /// Inclusive high end of the bucket's value range.
    pub high: u64,
    /// Count recorded in this bucket.
    pub count: u64,
    /// Saturating running total through this bucket.
    pub cumulative: u64,
}

/// Iterator over every bucket (empty ones included), lowest
/// value first.
#[derive(Debug)]
pub struct Buckets<'a, C: Counter> {
    config: Config,
    counts: &'a [C],
    index: usize,
    cumulative: u64,
}

impl<'a, C: Counter> Buckets<'a, C> {
    /// Build the iterator; internal (reached via the
    /// histogram types' `buckets()`).
    pub(crate) fn new(config: Config, counts: &'a [C]) -> Self {
        Buckets {
            config,
            counts,
            index: 0,
            cumulative: 0,
        }
    }
}

impl<C: Counter> Iterator for Buckets<'_, C> {
    type Item = Bucket;

    fn next(&mut self) -> Option<Bucket> {
        let c = self.counts.get(self.index)?;
        let count = c.to_u64();
        self.cumulative = self.cumulative.saturating_add(count);
        let (low, high) = self.config.value_range(self.index);
        self.index += 1;
        Some(Bucket {
            low,
            high,
            count,
            cumulative: self.cumulative,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let rest = self.counts.len() - self.index;
        (rest, Some(rest))
    }
}

impl<C: Counter> ExactSizeIterator for Buckets<'_, C> {}

/// Value at quantile `q` in `[0.0, 1.0]`: the **upper bound**
/// of the bucket holding the rank-`ceil(q·total)` recorded
/// value (matching hdrhistogram's highest-equivalent
/// convention; one-sided error ≤ 2⁻ᵍ).
///
/// `None` when the histogram is empty or `q` is outside
/// `[0.0, 1.0]` (NaN included). No `std` float intrinsics:
/// the ceil is done by integer compare.
pub(crate) fn quantile_of<C: Counter>(
    config: &Config,
    counts: &[C],
    total: u64,
    q: f64,
) -> Option<u64> {
    if total == 0 || !(0.0..=1.0).contains(&q) {
        return None;
    }
    // rank = clamp(ceil(q * total), 1, total) without f64::ceil.
    let rank_f = q * (total as f64);
    let mut rank = rank_f as u64;
    if (rank as f64) < rank_f {
        rank += 1;
    }
    let rank = rank.clamp(1, total);

    let mut cumulative = 0u64;
    for (i, c) in counts.iter().enumerate() {
        cumulative = cumulative.saturating_add(c.to_u64());
        if cumulative >= rank {
            let (_, high) = config.value_range(i);
            return Some(high);
        }
    }
    // Unreachable when `total` matches the counts (invariant);
    // fall back to the top bucket rather than panic.
    let (_, high) = config.value_range(counts.len() - 1);
    Some(high)
}

/// Merge `src` counts into `dst` (saturating), configs must
/// match. Returns the count actually added to `dst`'s total.
pub(crate) fn merge_into<C: Counter>(
    dst_config: &Config,
    dst_counts: &mut [C],
    src_config: &Config,
    src_counts: &[C],
) -> Result<u64, Error> {
    if dst_config != src_config {
        return Err(Error::ConfigMismatch);
    }
    let mut added = 0u64;
    for (d, s) in dst_counts.iter_mut().zip(src_counts.iter()) {
        let n = s.to_u64();
        *d = d.sat_add(n);
        added = added.saturating_add(n);
    }
    Ok(added)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // OK: tests panic on setup failure by design
mod tests {
    use crate::{Config, Error, Histogram, HistogramArray};

    /// 1..=100 recorded once each in an exact-region config:
    /// quantiles are exact order statistics.
    #[test]
    fn quantile_exact_region() {
        let c = Config::new(7, 20).unwrap();
        let mut counts = [0u32; 1792];
        let mut h = Histogram::new(c, &mut counts).unwrap();
        for v in 1..=100u64 {
            h.record(v);
        }
        assert_eq!(h.quantile(0.0), Some(1));
        assert_eq!(h.quantile(0.5), Some(50));
        assert_eq!(h.quantile(0.90), Some(90));
        assert_eq!(h.quantile(1.0), Some(100));
    }

    #[test]
    fn quantile_edge_cases() {
        let c = Config::new(2, 8).unwrap();
        let mut counts = [0u32; 28];
        let mut h = Histogram::new(c, &mut counts).unwrap();
        assert_eq!(h.quantile(0.5), None); // empty
        h.record(10);
        assert_eq!(h.quantile(-0.1), None);
        assert_eq!(h.quantile(1.1), None);
        assert_eq!(h.quantile(f64::NAN), None);
    }

    /// In the log region the quantile is the bucket's upper
    /// bound containing the ranked value.
    #[test]
    fn quantile_upper_bound_convention() {
        let c = Config::new(2, 10).unwrap();
        let mut counts = [0u32; 36];
        let mut h = Histogram::new(c, &mut counts).unwrap();
        h.record(1000); // single value, above exact region
        let idx = c.index_for(1000);
        let (low, high) = c.value_range(idx);
        assert!(low <= 1000 && 1000 <= high);
        assert_eq!(h.quantile(0.5), Some(high));
    }

    #[test]
    fn buckets_cumulative_and_partition() {
        let c = Config::new(2, 8).unwrap();
        let mut counts = [0u32; 28];
        let mut h = Histogram::new(c, &mut counts).unwrap();
        for v in [0u64, 3, 3, 9, 100, 255] {
            h.record(v);
        }
        let buckets: std::vec::Vec<crate::Bucket> = h.buckets().collect();
        assert_eq!(buckets.len(), c.total_buckets());
        assert_eq!(buckets.last().unwrap().cumulative, h.total());
        // Contiguous partition, cumulative monotone.
        for w in buckets.windows(2) {
            assert_eq!(w[0].high + 1, w[1].low);
            assert!(w[0].cumulative <= w[1].cumulative);
        }
        // Counts land where index_for says.
        assert_eq!(buckets[c.index_for(3)].count, 2);
    }

    #[test]
    fn merge_sums_and_mismatch() {
        let c = Config::new(3, 12).unwrap();
        let mut a_counts = [0u32; 80];
        let mut b_counts = [0u32; 80];
        let mut a = Histogram::new(c, &mut a_counts).unwrap();
        let mut b = Histogram::new(c, &mut b_counts).unwrap();
        for v in [1u64, 500, 4000] {
            a.record(v);
            b.record(v);
            b.record(v);
        }
        a.merge_from(&b).unwrap();
        assert_eq!(a.total(), 9);
        assert_eq!(a.count_at(c.index_for(500)), Some(3));

        let c2 = Config::new(2, 12).unwrap();
        let mut other_counts = [0u32; 44];
        let other = Histogram::new(c2, &mut other_counts).unwrap();
        assert!(matches!(a.merge_from(&other), Err(Error::ConfigMismatch)));
    }

    #[test]
    fn array_analysis_matches_borrowed() {
        let c = Config::new(3, 12).unwrap();
        let mut owned = HistogramArray::<80>::new(c).unwrap();
        let mut storage = [0u32; 80];
        let mut borrowed = Histogram::new(c, &mut storage).unwrap();
        for v in [0u64, 7, 8, 100, 3000, 4095] {
            owned.record(v);
            borrowed.record(v);
        }
        for q in [0.0, 0.25, 0.5, 0.9, 0.99, 1.0] {
            assert_eq!(owned.quantile(q), borrowed.quantile(q), "q={q}");
        }
        let mut owned2 = HistogramArray::<80>::new(c).unwrap();
        owned2.merge_from(&owned).unwrap();
        assert_eq!(owned2.total(), owned.total());
    }
}
