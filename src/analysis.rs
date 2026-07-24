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
pub struct Buckets<'a, Cnt: Counter> {
    config: Config,
    counts: &'a [Cnt],
    index: usize,
    cumulative: u64,
}

impl<'a, Cnt: Counter> Buckets<'a, Cnt> {
    /// Build the iterator; internal (reached via the
    /// histogram types' `buckets()`).
    pub(crate) fn new(config: Config, counts: &'a [Cnt]) -> Self {
        Buckets {
            config,
            counts,
            index: 0,
            cumulative: 0,
        }
    }
}

impl<Cnt: Counter> Iterator for Buckets<'_, Cnt> {
    type Item = Bucket;

    fn next(&mut self) -> Option<Bucket> {
        let cnt = self.counts.get(self.index)?;
        let count = cnt.to_u64();
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

impl<Cnt: Counter> ExactSizeIterator for Buckets<'_, Cnt> {}

/// Value at quantile `q` in `[0.0, 1.0]`: the **upper bound**
/// of the bucket holding the rank-`ceil(q·total)` recorded
/// value (matching hdrhistogram's highest-equivalent
/// convention; one-sided error ≤ 2⁻ᵍ).
///
/// `None` when the histogram is empty or `fraction` is outside
/// `[0.0, 1.0]` (NaN included). No `std` float intrinsics:
/// the ceil is done by integer compare. O(buckets), two
/// passes: `total` is summed from the counts first (nothing
/// tracks it on the record path).
pub(crate) fn quantile_of<Cnt: Counter>(
    config: &Config,
    counts: &[Cnt],
    fraction: f64,
) -> Option<u64> {
    let total = crate::histogram::total_of(counts);
    if total == 0 || !(0.0..=1.0).contains(&fraction) {
        return None;
    }
    // rank = clamp(ceil(fraction * total), 1, total) without f64::ceil.
    let rank_f = fraction * (total as f64);
    let mut rank = rank_f as u64;
    if (rank as f64) < rank_f {
        rank += 1;
    }
    let rank = rank.clamp(1, total);

    let mut cumulative = 0u64;
    for (index, cnt) in counts.iter().enumerate() {
        cumulative = cumulative.saturating_add(cnt.to_u64());
        if cumulative >= rank {
            let (_, high) = config.value_range(index);
            return Some(high);
        }
    }
    // Unreachable: `total` is summed from these same counts,
    // so some bucket reaches `rank ≤ total`; fall back to the
    // top bucket rather than panic.
    let (_, high) = config.value_range(counts.len() - 1);
    Some(high)
}

/// Merge `src` counts into `dst` (saturating), configs must
/// match.
pub(crate) fn merge_into<Cnt: Counter>(
    dst_config: &Config,
    dst_counts: &mut [Cnt],
    src_config: &Config,
    src_counts: &[Cnt],
) -> Result<(), Error> {
    if dst_config != src_config {
        return Err(Error::ConfigMismatch);
    }
    for (dst_cnt, src_cnt) in dst_counts.iter_mut().zip(src_counts.iter()) {
        *dst_cnt = dst_cnt.sat_add(src_cnt.to_u64());
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // OK: tests panic on setup failure by design
mod tests {
    use crate::{Config, Error, Histogram, HistogramArray};

    /// Build a config at compile time; invalid powers are a
    /// test-authoring error, so they fail the build rather
    /// than the run.
    const fn cfg(grouping_power: u8, max_value_power: u8) -> Config {
        match Config::new(grouping_power, max_value_power) {
            Ok(config) => config,
            Err(_) => panic!("invalid test config"),
        }
    }

    /// 1..=100 recorded once each in an exact-region config:
    /// quantiles are exact order statistics.
    #[test]
    fn quantile_exact_region() {
        const CFG: Config = cfg(7, 20);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        for value in 1..=100u64 {
            hist.record(value);
        }
        assert_eq!(hist.quantile(0.0), Some(1));
        assert_eq!(hist.quantile(0.5), Some(50));
        assert_eq!(hist.quantile(0.90), Some(90));
        assert_eq!(hist.quantile(1.0), Some(100));
    }

    #[test]
    fn quantile_edge_cases() {
        const CFG: Config = cfg(2, 8);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        assert_eq!(hist.quantile(0.5), None); // empty
        hist.record(10);
        assert_eq!(hist.quantile(-0.1), None);
        assert_eq!(hist.quantile(1.1), None);
        assert_eq!(hist.quantile(f64::NAN), None);
    }

    /// In the log region the quantile is the bucket's upper
    /// bound containing the ranked value.
    #[test]
    fn quantile_upper_bound_convention() {
        const CFG: Config = cfg(2, 10);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        hist.record(1000); // single value, above exact region
        let idx = CFG.index_for(1000);
        let (low, high) = CFG.value_range(idx);
        assert!(low <= 1000 && 1000 <= high);
        assert_eq!(hist.quantile(0.5), Some(high));
    }

    #[test]
    fn buckets_cumulative_and_partition() {
        const CFG: Config = cfg(2, 8);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        for value in [0u64, 3, 3, 9, 100, 255] {
            hist.record(value);
        }
        let buckets: std::vec::Vec<crate::Bucket> = hist.buckets().collect();
        assert_eq!(buckets.len(), BUCKETS);
        assert_eq!(buckets.last().unwrap().cumulative, hist.total());
        // Contiguous partition, cumulative monotone.
        for pair in buckets.windows(2) {
            assert_eq!(pair[0].high + 1, pair[1].low);
            assert!(pair[0].cumulative <= pair[1].cumulative);
        }
        // Counts land where index_for says.
        assert_eq!(buckets[CFG.index_for(3)].count, 2);
    }

    #[test]
    fn merge_sums_and_mismatch() {
        const CFG: Config = cfg(3, 12);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts_a = [0u32; BUCKETS];
        let mut counts_b = [0u32; BUCKETS];
        let mut hist_a = Histogram::new(CFG, &mut counts_a).unwrap();
        let mut hist_b = Histogram::new(CFG, &mut counts_b).unwrap();
        for value in [1u64, 500, 4000] {
            hist_a.record(value);
            hist_b.record(value);
            hist_b.record(value);
        }
        hist_a.merge_from(&hist_b).unwrap();
        assert_eq!(hist_a.total(), 9);
        assert_eq!(hist_a.count_at(CFG.index_for(500)), Some(3));

        const OTHER_CFG: Config = cfg(2, 12);
        const OTHER_BUCKETS: usize = OTHER_CFG.total_buckets();
        let mut other_counts = [0u32; OTHER_BUCKETS];
        let other = Histogram::new(OTHER_CFG, &mut other_counts).unwrap();
        assert!(matches!(
            hist_a.merge_from(&other),
            Err(Error::ConfigMismatch)
        ));
    }

    #[test]
    fn array_analysis_matches_borrowed() {
        const CFG: Config = cfg(3, 12);
        const BUCKETS: usize = CFG.total_buckets();
        let mut owned = HistogramArray::<BUCKETS>::new(CFG).unwrap();
        let mut storage = [0u32; BUCKETS];
        let mut borrowed = Histogram::new(CFG, &mut storage).unwrap();
        for value in [0u64, 7, 8, 100, 3000, 4095] {
            owned.record(value);
            borrowed.record(value);
        }
        for fraction in [0.0, 0.25, 0.5, 0.9, 0.99, 1.0] {
            assert_eq!(
                owned.quantile(fraction),
                borrowed.quantile(fraction),
                "q={fraction}"
            );
        }
        let mut owned2 = HistogramArray::<BUCKETS>::new(CFG).unwrap();
        owned2.merge_from(&owned).unwrap();
        assert_eq!(owned2.total(), owned.total());
    }
}
