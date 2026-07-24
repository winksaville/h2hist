//! Constants shared across the test, bench, and demo
//! consumers.
//!
//! - One home for every value more than one consumer file
//!   depends on: the histogram config, the PRNG seed, and the
//!   oracle's precision. A literal repeated across consumers
//!   can drift in one and not the others; a shared const
//!   cannot.
//! - A value only one file uses lives in that file, not here —
//!   the stream's shape in [`super::stream`], a consumer's
//!   sample count / rep count / config sweep in the consumer.

use h2hist::Config;

/// Seed for every deterministic stream, so the test, bench,
/// and demo all describe the same values.
pub const SEED: u64 = 42;

/// Grouping power g of the shared config: relative value error
/// ≤ 2⁻⁷ ≈ 0.78%.
pub const GROUPING_POWER: u8 = 7;

/// Max value power n of the shared config: values up to 2³⁰−1
/// ticks are trackable.
pub const MAX_VALUE_POWER: u8 = 30;

/// The config the demo, the bench, and the quantile-parity
/// test all record into.
///
/// A `const` rather than a runtime `Config::new`, so storage
/// can be sized at compile time from [`BUCKETS`] and the
/// powers cannot disagree with the printed header.
pub const CFG: Config = match Config::new(GROUPING_POWER, MAX_VALUE_POWER) {
    Ok(config) => config,
    Err(_) => panic!("dev config powers are invalid"),
};

/// Counts-storage length [`CFG`] requires — the array size for
/// a histogram over it.
pub const BUCKETS: usize = CFG.total_buckets();

/// `hdrhistogram` significant figures paired with
/// [`GROUPING_POWER`], so a comparison against it runs at like
/// precision: 2 sigfigs is ≤ 1% relative error against g=7's
/// ≤ 0.78%.
pub const HDR_SIGFIG: u8 = 2;
