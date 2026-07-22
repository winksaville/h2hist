//! The borrowed-storage histogram and its record path.
//!
//! - `Histogram` borrows its counts slice from the caller
//!   (static buffer, stack array, or heap under `std`), so
//!   storage stays detachable — the future buffer-swap
//!   servicing model needs exactly that.
//! - `record` is the one hot operation: index (a compare, a
//!   clz, two shifts) and a saturating increment. No floats,
//!   no allocation, no panics.

use crate::config::{Config, Error};
use crate::counter::Counter;

/// Shared record-path core for the borrowed and owned types:
/// index `value`, saturating-add `n` into its bucket, bump
/// `total`. The `get_mut` guard makes the panic path
/// unreachable (index invariant proven by the config tests).
#[inline]
pub(crate) fn record_into<C: Counter>(
    config: &Config,
    counts: &mut [C],
    total: &mut u64,
    value: u64,
    n: u64,
) {
    let idx = config.index_for(value);
    if let Some(c) = counts.get_mut(idx) {
        *c = c.sat_add(n);
        *total = total.saturating_add(n);
    }
}

/// A log-linear histogram over caller-supplied counts storage.
///
/// - `config` — the h2 powers and index math.
/// - `counts` — exactly `config.total_buckets()` counters.
/// - `total` — saturating sum of all recorded counts (u64),
///   maintained on the record path for O(1) rank math later.
#[derive(Debug)]
pub struct Histogram<'a, C: Counter = u32> {
    config: Config,
    total: u64,
    counts: &'a mut [C],
}

impl<'a, C: Counter> Histogram<'a, C> {
    /// Bind a config to its counts storage.
    ///
    /// - `counts.len()` must equal `config.total_buckets()`
    ///   ([`Error::StorageLen`] otherwise).
    /// - Storage is not zeroed here: pre-zeroed storage (or a
    ///   reused slice from a previous run) is the caller's
    ///   contract; `total` is recomputed from the counts so a
    ///   handed-back slice stays consistent.
    pub fn new(config: Config, counts: &'a mut [C]) -> Result<Self, Error> {
        if counts.len() != config.total_buckets() {
            return Err(Error::StorageLen);
        }
        let mut total = 0u64;
        for c in counts.iter() {
            total = total.saturating_add(c.to_u64());
        }
        Ok(Histogram {
            config,
            total,
            counts,
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

    /// Record one occurrence of `value`. O(1), never panics;
    /// over-range values clamp into the top bucket and a full
    /// counter saturates.
    #[inline]
    pub fn record(&mut self, value: u64) {
        self.record_n(value, 1);
    }

    /// Record `n` occurrences of `value`; same semantics as
    /// [`record`](Histogram::record).
    #[inline]
    pub fn record_n(&mut self, value: u64, n: u64) {
        record_into(&self.config, self.counts, &mut self.total, value, n);
    }

    /// Count in bucket `index`, widened to u64; `None` past
    /// the last bucket.
    pub fn count_at(&self, index: usize) -> Option<u64> {
        self.counts.get(index).map(|c| c.to_u64())
    }

    /// Release the borrow and hand the counts slice back —
    /// the swap-model hand-off shape.
    pub fn into_counts(self) -> &'a mut [C] {
        self.counts
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // OK: tests panic on setup failure by design
mod tests {
    use super::*;

    /// A (config, storage) pair for tests.
    fn cfg(g: u8, n: u8) -> Config {
        Config::new(g, n).unwrap()
    }

    #[test]
    fn storage_len_checked() {
        let c = cfg(2, 8);
        let mut too_short = [0u32; 3];
        assert!(matches!(
            Histogram::new(c, &mut too_short),
            Err(Error::StorageLen)
        ));
        let mut right = [0u32; 28]; // (8-2+1)<<2
        assert!(Histogram::new(c, &mut right).is_ok());
    }

    #[test]
    fn record_exact_region() {
        let c = cfg(3, 10);
        let mut counts = [0u32; 64];
        let mut h = Histogram::new(c, &mut counts).unwrap();
        h.record(0);
        h.record(5);
        h.record(5);
        assert_eq!(h.total(), 3);
        assert_eq!(h.count_at(0), Some(1));
        assert_eq!(h.count_at(5), Some(2));
        assert_eq!(h.count_at(6), Some(0));
    }

    #[test]
    fn record_n_and_clamp() {
        let c = cfg(2, 8);
        let mut counts = [0u32; 28];
        let mut h = Histogram::new(c, &mut counts).unwrap();
        h.record_n(1_000_000, 7); // way over max_value = 255
        assert_eq!(h.total(), 7);
        assert_eq!(h.count_at(c.total_buckets() - 1), Some(7));
    }

    #[test]
    fn counter_saturates_not_wraps() {
        let c = cfg(0, 1); // 2 buckets, minimal config
        let mut counts = [u8::MAX - 1, 0u8];
        let mut h = Histogram::new(c, &mut counts).unwrap();
        h.record(0);
        h.record(0);
        h.record(0);
        assert_eq!(h.count_at(0), Some(u8::MAX as u64));
        // total still counts every record() call.
        assert_eq!(h.total(), (u8::MAX - 1) as u64 + 3);
    }

    #[test]
    fn rebind_recomputes_total() {
        let c = cfg(2, 8);
        let mut counts = [0u32; 28];
        {
            let mut h = Histogram::new(c, &mut counts).unwrap();
            h.record(3);
            h.record(200);
        }
        // Rebind the same storage: total must be rebuilt.
        let h = Histogram::new(c, &mut counts).unwrap();
        assert_eq!(h.total(), 2);
    }

    #[test]
    fn into_counts_hands_storage_back() {
        let c = cfg(2, 8);
        let mut counts = [0u32; 28];
        let mut h = Histogram::new(c, &mut counts).unwrap();
        h.record(3);
        let slice = h.into_counts();
        assert_eq!(slice[3], 1);
    }
}
