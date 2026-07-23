//! The borrowed-storage histogram and its record path.
//!
//! - `Histogram` borrows its counts slice from the caller
//!   (static buffer, stack array, or heap under `std`), so
//!   storage stays detachable — the future buffer-swap
//!   servicing model needs exactly that.
//! - `record` is the one hot operation: index (a compare, a
//!   clz, two shifts) and a saturating increment. No floats,
//!   no allocation, no panics.

use crate::analysis::{Buckets, merge_into, quantile_of};
use crate::config::{Config, Error};
use crate::counter::Counter;

/// Shared record-path core for the borrowed and owned types:
/// index `value`, saturating-add `count` into its bucket.
/// Nothing else — totals are computed at read time so the hot
/// path stays minimal. The `get_mut` guard makes the panic
/// path unreachable (index invariant proven by the config
/// tests).
#[inline]
pub(crate) fn record_into<Cnt: Counter>(
    config: &Config,
    counts: &mut [Cnt],
    value: u64,
    count: u64,
) {
    let idx = config.index_for(value);
    if let Some(cnt) = counts.get_mut(idx) {
        *cnt = cnt.sat_add(count);
    }
}

/// Saturating sum of a counts slice — the read-time `total`
/// shared by the borrowed and owned types.
pub(crate) fn total_of<Cnt: Counter>(counts: &[Cnt]) -> u64 {
    let mut total = 0u64;
    for cnt in counts {
        total = total.saturating_add(cnt.to_u64());
    }
    total
}

/// A log-linear histogram over caller-supplied counts storage.
///
/// - `config` — the h2 powers and index math.
/// - `counts` — exactly `config.total_buckets()` counters.
/// - No derived state: totals are computed from the counts at
///   read time, keeping the record path minimal.
#[derive(Debug)]
pub struct Histogram<'a, Cnt: Counter = u32> {
    config: Config,
    counts: &'a mut [Cnt],
}

impl<'a, Cnt: Counter> Histogram<'a, Cnt> {
    /// Bind a config to its counts storage.
    ///
    /// - `counts.len()` must equal `config.total_buckets()`
    ///   ([`Error::StorageLen`] otherwise).
    /// - Storage is not zeroed here: pre-zeroed storage (or a
    ///   reused slice from a previous run) is the caller's
    ///   contract; totals are read from the counts on demand,
    ///   so a handed-back slice is consistent as-is.
    pub fn new(config: Config, counts: &'a mut [Cnt]) -> Result<Self, Error> {
        if counts.len() != config.total_buckets() {
            return Err(Error::StorageLen);
        }
        Ok(Histogram { config, counts })
    }

    /// The config this histogram was built with.
    pub fn config(&self) -> Config {
        self.config
    }

    /// Saturating sum of all bucket counts. O(buckets):
    /// computed at read time to keep the record path minimal,
    /// so a saturated counter's overflow is not reflected here.
    pub fn total(&self) -> u64 {
        total_of(self.counts)
    }

    /// Record one occurrence of `value`. O(1), never panics;
    /// over-range values clamp into the top bucket and a full
    /// counter saturates.
    #[inline]
    pub fn record(&mut self, value: u64) {
        self.record_n(value, 1);
    }

    /// Record `count` occurrences of `value`; same semantics
    /// as [`record`](Histogram::record).
    #[inline]
    pub fn record_n(&mut self, value: u64, count: u64) {
        record_into(&self.config, self.counts, value, count);
    }

    /// Count in bucket `index`, widened to u64; `None` past
    /// the last bucket.
    pub fn count_at(&self, index: usize) -> Option<u64> {
        self.counts.get(index).map(|cnt| cnt.to_u64())
    }

    /// Release the borrow and hand the counts slice back —
    /// the swap-model hand-off shape.
    pub fn into_counts(self) -> &'a mut [Cnt] {
        self.counts
    }

    /// Iterate every bucket lowest-first, with cumulative
    /// counts (the band-table building block).
    pub fn buckets(&self) -> Buckets<'_, Cnt> {
        Buckets::new(self.config, self.counts)
    }

    /// Value at quantile `fraction` in `[0.0, 1.0]`: the upper
    /// bound of the bucket holding the rank-`ceil(fraction·total)`
    /// value (hdrhistogram's highest-equivalent convention,
    /// one-sided error ≤ 2⁻ᵍ). `None` when empty or `fraction`
    /// is out of range (NaN included).
    pub fn quantile(&self, fraction: f64) -> Option<u64> {
        quantile_of(&self.config, self.counts, fraction)
    }

    /// Merge `other`'s counts into `self` (saturating);
    /// configs must be identical.
    pub fn merge_from(&mut self, other: &Histogram<'_, Cnt>) -> Result<(), Error> {
        merge_into(&self.config, self.counts, &other.config, other.counts)
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
    /// Counts length [`CFG`] requires — derived, so changing
    /// the powers cannot leave a stale hand-computed length.
    const BUCKETS: usize = CFG.total_buckets();

    #[test]
    fn storage_len_checked() {
        let mut too_short = [0u32; BUCKETS - 1];
        assert!(matches!(
            Histogram::new(CFG, &mut too_short),
            Err(Error::StorageLen)
        ));
        let mut right = [0u32; BUCKETS];
        assert!(Histogram::new(CFG, &mut right).is_ok());
    }

    #[test]
    fn record_exact_region() {
        const EXACT_CFG: Config = cfg(3, 10);
        const EXACT_BUCKETS: usize = EXACT_CFG.total_buckets();
        let mut counts = [0u32; EXACT_BUCKETS];
        let mut hist = Histogram::new(EXACT_CFG, &mut counts).unwrap();
        hist.record(0);
        hist.record(5);
        hist.record(5);
        assert_eq!(hist.total(), 3);
        assert_eq!(hist.count_at(0), Some(1));
        assert_eq!(hist.count_at(5), Some(2));
        assert_eq!(hist.count_at(6), Some(0));
    }

    #[test]
    fn record_n_and_clamp() {
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        hist.record_n(1_000_000, 7); // way over max_value = 255
        assert_eq!(hist.total(), 7);
        assert_eq!(hist.count_at(BUCKETS - 1), Some(7));
    }

    #[test]
    fn counter_saturates_not_wraps() {
        const TINY_CFG: Config = cfg(0, 1); // 2 buckets, minimal config
        let mut counts = [u8::MAX - 1, 0u8];
        let mut hist = Histogram::new(TINY_CFG, &mut counts).unwrap();
        hist.record(0);
        hist.record(0);
        hist.record(0);
        assert_eq!(hist.count_at(0), Some(u8::MAX as u64));
        // total is the sum of counts, so it saturates with them.
        assert_eq!(hist.total(), u8::MAX as u64);
    }

    #[test]
    fn rebind_total_from_counts() {
        let mut counts = [0u32; BUCKETS];
        {
            let mut hist = Histogram::new(CFG, &mut counts).unwrap();
            hist.record(3);
            hist.record(200);
        }
        // Rebind the same storage: total reads the counts.
        let hist = Histogram::new(CFG, &mut counts).unwrap();
        assert_eq!(hist.total(), 2);
    }

    #[test]
    fn into_counts_hands_storage_back() {
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        hist.record(3);
        let slice = hist.into_counts();
        assert_eq!(slice[3], 1);
    }
}
