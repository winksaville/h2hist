//! h2 configuration and the log-linear index math.
//!
//! - `Config` holds the two powers `(grouping_power,
//!   max_value_power)` and derives everything else as
//!   `const fn`: bucket count, value→index, index→value range.
//! - The scheme: values below `2^(g+1)` are exact (width-1
//!   buckets); each power-of-two range above holds `2^g`
//!   equal-width buckets, so relative error ≤ `2^-g`.

/// Errors from configuration validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// `max_value_power` is outside `1..=64`.
    MaxValuePower,
    /// `grouping_power` is not below `max_value_power`, or
    /// exceeds 62 (keeps every internal shift in u64 range).
    GroupingPower,
    /// `(n - g + 1) << g` exceeds `u32::MAX` buckets — far
    /// beyond any practical footprint.
    TooManyBuckets,
    /// Counts storage length differs from
    /// `Config::total_buckets()`.
    StorageLen,
    /// Operation requires identical configs (e.g. merge).
    ConfigMismatch,
    /// Band-ladder tail depth outside `2..=19` — below 2 the
    /// tail vanishes into the deciles, above 19 the
    /// `10^depth` fence math leaves u64.
    BandDepth,
}

/// The two h2 powers plus the index math they induce.
///
/// - `grouping_power` (g): `2^g` buckets per power-of-two
///   range; relative value error ≤ `2^-g`.
/// - `max_value_power` (n): max trackable value is `2^n - 1`;
///   larger values clamp into the top bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    grouping_power: u8,
    max_value_power: u8,
}

impl Config {
    /// Validate and build a config; see [`Error`] for the
    /// rejection rules.
    pub const fn new(grouping_power: u8, max_value_power: u8) -> Result<Config, Error> {
        if max_value_power < 1 || max_value_power > 64 {
            return Err(Error::MaxValuePower);
        }
        if grouping_power >= max_value_power || grouping_power > 62 {
            return Err(Error::GroupingPower);
        }
        let buckets = ((max_value_power - grouping_power) as u64 + 1) << grouping_power;
        if buckets > u32::MAX as u64 {
            return Err(Error::TooManyBuckets);
        }
        Ok(Config {
            grouping_power,
            max_value_power,
        })
    }

    /// The grouping power g.
    #[inline]
    pub const fn grouping_power(&self) -> u8 {
        self.grouping_power
    }

    /// The max value power n.
    #[inline]
    pub const fn max_value_power(&self) -> u8 {
        self.max_value_power
    }

    /// Largest trackable value, `2^n - 1`.
    #[inline]
    pub const fn max_value(&self) -> u64 {
        u64::MAX >> (64 - self.max_value_power as u32)
    }

    /// Number of buckets, `(n - g + 1) * 2^g` — the exact
    /// counts-storage length this config requires.
    #[inline]
    pub const fn total_buckets(&self) -> usize {
        (((self.max_value_power - self.grouping_power) as usize) + 1) << self.grouping_power
    }

    /// Bucket index for a value; values above
    /// [`max_value`](Config::max_value) clamp into the top
    /// bucket. O(1): a compare, a clz, two shifts.
    ///
    /// The exact-region test runs first so the common small
    /// value pays one compare and no clamp; over-max values
    /// are always ≥ `2^(g+1)` (g < n), so clamping only on
    /// the log path is equivalent.
    #[inline]
    pub const fn index_for(&self, value: u64) -> usize {
        if value < (1u64 << (self.grouping_power + 1)) {
            return value as usize;
        }
        let clamped = if value > self.max_value() {
            self.max_value()
        } else {
            value
        };
        let power = 63 - clamped.leading_zeros() as u8;
        let log_bucket = power - self.grouping_power;
        ((log_bucket as usize) << self.grouping_power)
            + ((clamped >> (power - self.grouping_power)) as usize)
    }

    /// Inclusive value range `(low, high)` of a bucket index.
    ///
    /// Callers must pass `index < total_buckets()`; the math
    /// is meaningless beyond it (no panic, garbage range).
    #[inline]
    pub const fn value_range(&self, index: usize) -> (u64, u64) {
        if index < (1usize << (self.grouping_power + 1)) {
            return (index as u64, index as u64);
        }
        let log_bucket = (index >> self.grouping_power) as u8 - 1;
        let offset = index - ((log_bucket as usize) << self.grouping_power);
        let low = (offset as u64) << log_bucket;
        (low, low + ((1u64 << log_bucket) - 1))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // OK: tests panic on setup failure by design
mod tests {
    use super::*;

    #[test]
    fn validation() {
        assert_eq!(Config::new(0, 0), Err(Error::MaxValuePower));
        assert_eq!(Config::new(0, 65), Err(Error::MaxValuePower));
        assert_eq!(Config::new(4, 4), Err(Error::GroupingPower));
        assert_eq!(Config::new(5, 4), Err(Error::GroupingPower));
        assert_eq!(Config::new(63, 64), Err(Error::GroupingPower));
        assert_eq!(Config::new(30, 64), Err(Error::TooManyBuckets));
        assert!(Config::new(0, 1).is_ok());
        assert!(Config::new(7, 40).is_ok());
        assert!(Config::new(62, 64).is_err()); // TooManyBuckets
    }

    #[test]
    fn known_sizes() {
        // The ARCHITECTURE.md size-table rows.
        let cases = [
            (2u8, 32u8, 124usize),
            (4, 36, 528),
            (6, 40, 2240),
            (7, 40, 4352),
            (10, 40, 31744),
        ];
        for (grouping_power, max_value_power, buckets) in cases {
            let config = Config::new(grouping_power, max_value_power).unwrap();
            assert_eq!(
                config.total_buckets(),
                buckets,
                "g={grouping_power} n={max_value_power}"
            );
        }
    }

    #[test]
    fn max_value() {
        assert_eq!(Config::new(2, 8).unwrap().max_value(), 255);
        assert_eq!(Config::new(2, 64).unwrap().max_value(), u64::MAX);
    }

    /// Walk every value of small configs: indices must start
    /// at 0, end at `total_buckets - 1`, be monotone
    /// non-decreasing, and agree with `value_range` — which
    /// must exactly partition `0..=max_value`.
    #[test]
    fn exhaustive_small_configs() {
        for grouping_power in 0u8..=3 {
            for max_value_power in (grouping_power + 1)..=10 {
                let config = Config::new(grouping_power, max_value_power).unwrap();
                let total = config.total_buckets();
                let mut prev_idx = 0usize;
                let mut next_expected_low = 0u64;
                for value in 0..=config.max_value() {
                    let idx = config.index_for(value);
                    assert!(
                        idx < total,
                        "g={grouping_power} n={max_value_power} v={value}"
                    );
                    assert!(
                        idx >= prev_idx,
                        "monotone g={grouping_power} n={max_value_power} v={value}"
                    );
                    let (low, high) = config.value_range(idx);
                    assert!(
                        low <= value && value <= high,
                        "g={grouping_power} n={max_value_power} v={value} idx={idx}"
                    );
                    if idx != prev_idx || value == 0 {
                        // First value of a bucket: partition check.
                        assert_eq!(
                            low, next_expected_low,
                            "g={grouping_power} n={max_value_power} v={value}"
                        );
                        assert_eq!(low, value, "bucket low is its first value");
                        next_expected_low = high + 1;
                    }
                    prev_idx = idx;
                }
                assert_eq!(
                    prev_idx,
                    total - 1,
                    "top value hits top bucket g={grouping_power} n={max_value_power}"
                );
                assert_eq!(
                    next_expected_low,
                    config.max_value() + 1,
                    "partition covers domain"
                );
            }
        }
    }

    #[test]
    fn over_range_clamps_to_top_bucket() {
        let config = Config::new(3, 10).unwrap();
        let top = config.total_buckets() - 1;
        assert_eq!(config.index_for(config.max_value()), top);
        assert_eq!(config.index_for(config.max_value() + 1), top);
        assert_eq!(config.index_for(u64::MAX), top);
    }

    #[test]
    fn n64_top_bucket() {
        let config = Config::new(2, 64).unwrap();
        assert_eq!(config.index_for(u64::MAX), config.total_buckets() - 1);
        let (low, high) = config.value_range(config.total_buckets() - 1);
        assert!(low <= high);
        assert_eq!(high, u64::MAX);
    }

    #[test]
    fn exact_region_is_exact() {
        let config = Config::new(4, 20).unwrap();
        for value in 0..(1u64 << 5) {
            assert_eq!(config.value_range(config.index_for(value)), (value, value));
        }
    }
}
