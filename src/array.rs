//! Owned inline-array histogram.
//!
//! - `HistogramArray<LEN, Cnt>` embeds its `[Cnt; LEN]` counts,
//!   for callers who want one self-contained value (a static, a
//!   struct field) instead of managing a separate slice.
//! - Stable Rust cannot derive `LEN` from a `Config`'s powers
//!   (`generic_const_exprs` is unstable), so `LEN` is explicit
//!   and checked at construction against
//!   `Config::total_buckets()`.

use crate::analysis::{Buckets, merge_into, quantile_of};
use crate::config::{Config, Error};
use crate::counter::Counter;
use crate::histogram::{Histogram, record_into, total_of};

/// A histogram owning its counts inline.
///
/// `LEN` must equal `config.total_buckets()`; compute it from a
/// `const` config:
///
/// ```
/// use h2hist::{Config, HistogramArray};
///
/// const CFG: Config = match Config::new(4, 36) {
///     Ok(config) => config,
///     Err(_) => panic!("invalid config"),
/// };
/// let mut hist = HistogramArray::<{ CFG.total_buckets() }>::new(CFG).unwrap();
/// hist.record(42);
/// assert_eq!(hist.total(), 1);
/// ```
#[derive(Debug)]
pub struct HistogramArray<const LEN: usize, Cnt: Counter = u32> {
    config: Config,
    counts: [Cnt; LEN],
}

impl<const LEN: usize, Cnt: Counter> HistogramArray<LEN, Cnt> {
    /// Build a zeroed histogram; [`Error::StorageLen`] if
    /// `LEN != config.total_buckets()`.
    pub fn new(config: Config) -> Result<Self, Error> {
        if config.total_buckets() != LEN {
            return Err(Error::StorageLen);
        }
        Ok(HistogramArray {
            config,
            counts: [Cnt::default(); LEN],
        })
    }

    /// The config this histogram was built with.
    pub fn config(&self) -> Config {
        self.config
    }

    /// Saturating sum of all bucket counts; semantics of
    /// [`Histogram::total`] (O(buckets), read-time).
    pub fn total(&self) -> u64 {
        total_of(&self.counts)
    }

    /// Record one occurrence of `value`; semantics of
    /// [`Histogram::record`].
    #[inline]
    pub fn record(&mut self, value: u64) {
        self.record_n(value, 1);
    }

    /// Record `count` occurrences of `value`; semantics of
    /// [`Histogram::record_n`].
    #[inline]
    pub fn record_n(&mut self, value: u64, count: u64) {
        record_into(&self.config, &mut self.counts, value, count);
    }

    /// Count in bucket `index`, widened to u64; `None` past
    /// the last bucket.
    pub fn count_at(&self, index: usize) -> Option<u64> {
        self.counts.get(index).map(|cnt| cnt.to_u64())
    }

    /// Iterate every bucket lowest-first, with cumulative
    /// counts (the band-table building block).
    pub fn buckets(&self) -> Buckets<'_, Cnt> {
        Buckets::new(self.config, &self.counts)
    }

    /// Value at quantile `fraction`; semantics of
    /// [`Histogram::quantile`].
    pub fn quantile(&self, fraction: f64) -> Option<u64> {
        quantile_of(&self.config, &self.counts, fraction)
    }

    /// Merge `other`'s counts into `self` (saturating);
    /// configs must be identical.
    pub fn merge_from(&mut self, other: &HistogramArray<LEN, Cnt>) -> Result<(), Error> {
        merge_into(&self.config, &mut self.counts, &other.config, &other.counts)
    }

    /// Borrow as the slice-backed [`Histogram`] view — one
    /// analysis surface for both storage shapes. O(1): the
    /// view is config + slice, no derived state to build.
    pub fn as_histogram(&mut self) -> Histogram<'_, Cnt> {
        #[allow(clippy::unwrap_used)]
        // OK: LEN == total_buckets() was checked in new(), the
        // only constructor, so rebinding cannot fail.
        Histogram::new(self.config, &mut self.counts).unwrap()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // OK: tests panic on setup failure by design
mod tests {
    use super::*;

    /// Build a config at compile time; invalid powers are a
    /// test-authoring error, so they fail the build rather
    /// than the run.
    const fn cfg(grouping_power: u8, max_value_power: u8) -> Config {
        match Config::new(grouping_power, max_value_power) {
            Ok(config) => config,
            Err(_) => panic!("invalid test config"),
        }
    }

    /// The config most tests here use.
    const CFG: Config = cfg(2, 8);
    /// Counts length [`CFG`] requires — derived, so the `LEN`
    /// type parameter cannot drift from the powers.
    const BUCKETS: usize = CFG.total_buckets();

    #[test]
    fn size_checked() {
        assert!(matches!(
            HistogramArray::<{ BUCKETS - 1 }>::new(CFG),
            Err(Error::StorageLen)
        ));
        assert!(HistogramArray::<BUCKETS>::new(CFG).is_ok());
    }

    #[test]
    fn record_matches_borrowed() {
        const CFG: Config = cfg(3, 12);
        const BUCKETS: usize = CFG.total_buckets();
        let mut owned = HistogramArray::<BUCKETS>::new(CFG).unwrap();
        let mut storage = [0u32; BUCKETS];
        let mut borrowed = Histogram::new(CFG, &mut storage).unwrap();
        for value in [0u64, 7, 8, 100, 3000, 4095, u64::MAX] {
            owned.record(value);
            borrowed.record(value);
        }
        assert_eq!(owned.total(), borrowed.total());
        for index in 0..BUCKETS {
            assert_eq!(
                owned.count_at(index),
                borrowed.count_at(index),
                "bucket {index}"
            );
        }
    }

    #[test]
    fn as_histogram_view() {
        let mut hist = HistogramArray::<BUCKETS>::new(CFG).unwrap();
        hist.record(3);
        hist.record(3);
        let view = hist.as_histogram();
        assert_eq!(view.total(), 2);
        assert_eq!(view.count_at(3), Some(2));
    }
}
