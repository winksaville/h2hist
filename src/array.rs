//! Owned inline-array histogram.
//!
//! - `HistogramArray<N, C>` embeds its `[C; N]` counts, for
//!   callers who want one self-contained value (a static, a
//!   struct field) instead of managing a separate slice.
//! - Stable Rust cannot derive `N` from a `Config`'s powers
//!   (`generic_const_exprs` is unstable), so `N` is explicit
//!   and checked at construction against
//!   `Config::total_buckets()`.

use crate::analysis::{Buckets, merge_into, quantile_of};
use crate::config::{Config, Error};
use crate::counter::Counter;
use crate::histogram::{Histogram, record_into};

/// A histogram owning its counts inline.
///
/// `N` must equal `config.total_buckets()`; compute it from a
/// `const` config:
///
/// ```
/// use histogram_no_std::{Config, HistogramArray};
///
/// const CFG: Config = match Config::new(4, 36) {
///     Ok(c) => c,
///     Err(_) => panic!("invalid config"),
/// };
/// let mut h = HistogramArray::<{ CFG.total_buckets() }>::new(CFG).unwrap();
/// h.record(42);
/// assert_eq!(h.total(), 1);
/// ```
#[derive(Debug)]
pub struct HistogramArray<const N: usize, C: Counter = u32> {
    config: Config,
    total: u64,
    counts: [C; N],
}

impl<const N: usize, C: Counter> HistogramArray<N, C> {
    /// Build a zeroed histogram; [`Error::StorageLen`] if
    /// `N != config.total_buckets()`.
    pub fn new(config: Config) -> Result<Self, Error> {
        if config.total_buckets() != N {
            return Err(Error::StorageLen);
        }
        Ok(HistogramArray {
            config,
            total: 0,
            counts: [C::default(); N],
        })
    }

    /// The config this histogram was built with.
    pub fn config(&self) -> Config {
        self.config
    }

    /// Saturating sum of all recorded counts.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Record one occurrence of `value`; semantics of
    /// [`Histogram::record`].
    #[inline]
    pub fn record(&mut self, value: u64) {
        self.record_n(value, 1);
    }

    /// Record `n` occurrences of `value`; semantics of
    /// [`Histogram::record_n`].
    #[inline]
    pub fn record_n(&mut self, value: u64, n: u64) {
        record_into(&self.config, &mut self.counts, &mut self.total, value, n);
    }

    /// Count in bucket `index`, widened to u64; `None` past
    /// the last bucket.
    pub fn count_at(&self, index: usize) -> Option<u64> {
        self.counts.get(index).map(|c| c.to_u64())
    }

    /// Iterate every bucket lowest-first, with cumulative
    /// counts (the band-table building block).
    pub fn buckets(&self) -> Buckets<'_, C> {
        Buckets::new(self.config, &self.counts)
    }

    /// Value at quantile `q`; semantics of
    /// [`Histogram::quantile`].
    pub fn quantile(&self, q: f64) -> Option<u64> {
        quantile_of(&self.config, &self.counts, self.total, q)
    }

    /// Merge `other`'s counts into `self` (saturating);
    /// configs must be identical.
    pub fn merge_from(&mut self, other: &HistogramArray<N, C>) -> Result<(), Error> {
        let added = merge_into(&self.config, &mut self.counts, &other.config, &other.counts)?;
        self.total = self.total.saturating_add(added);
        Ok(())
    }

    /// Borrow as the slice-backed [`Histogram`] view — one
    /// analysis surface for both storage shapes. O(buckets)
    /// (the view recomputes `total` from the counts).
    pub fn as_histogram(&mut self) -> Histogram<'_, C> {
        #[allow(clippy::unwrap_used)]
        // OK: N == total_buckets() was checked in new(), the
        // only constructor, so rebinding cannot fail.
        Histogram::new(self.config, &mut self.counts).unwrap()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // OK: tests panic on setup failure by design
mod tests {
    use super::*;

    #[test]
    fn size_checked() {
        let c = Config::new(2, 8).unwrap(); // 28 buckets
        assert!(matches!(
            HistogramArray::<27>::new(c),
            Err(Error::StorageLen)
        ));
        assert!(HistogramArray::<28>::new(c).is_ok());
    }

    #[test]
    fn record_matches_borrowed() {
        let c = Config::new(3, 12).unwrap();
        let mut owned = HistogramArray::<80>::new(c).unwrap();
        let mut storage = [0u32; 80];
        let mut borrowed = Histogram::new(c, &mut storage).unwrap();
        for v in [0u64, 7, 8, 100, 3000, 4095, u64::MAX] {
            owned.record(v);
            borrowed.record(v);
        }
        assert_eq!(owned.total(), borrowed.total());
        for i in 0..c.total_buckets() {
            assert_eq!(owned.count_at(i), borrowed.count_at(i), "bucket {i}");
        }
    }

    #[test]
    fn as_histogram_view() {
        let c = Config::new(2, 8).unwrap();
        let mut h = HistogramArray::<28>::new(c).unwrap();
        h.record(3);
        h.record(3);
        let view = h.as_histogram();
        assert_eq!(view.total(), 2);
        assert_eq!(view.count_at(3), Some(2));
    }
}
