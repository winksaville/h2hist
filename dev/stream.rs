//! Synthetic heavy-tailed latency stream.
//!
//! - Shape: a body of [`BASE_LO`]`..`[`BASE_LO`]`+`[`BASE_SPAN`]
//!   ticks, with roughly 1 draw in [`TAIL_ODDS`] leaving it — a
//!   near tail stretching up to [`NEAR_TAIL_MULT`]× and a far
//!   tail up to [`FAR_TAIL_MULT`]×. That is the distribution a
//!   latency histogram exists for: a tight mode plus decades of
//!   tail.
//! - Infinite; consumers take the count they want. Seeded, so
//!   the test's stream, the bench's stream, and the demo's
//!   stream are the same values when seeded the same.
//!
//! The shape constants live here rather than in
//! [`super::consts`] because only this file reads them.

use super::rng::SplitMix64;

/// Low end of the stream's body values.
const BASE_LO: u64 = 50;

/// Width of the stream's uniformly-drawn body.
const BASE_SPAN: u64 = 100;

/// One draw in `TAIL_ODDS` leaves the body for a tail.
const TAIL_ODDS: u64 = 1_000;

/// Draws `1..=NEAR_TAIL_SHARE` (of [`TAIL_ODDS`]) take the near
/// tail; draw `0` takes the far tail.
const NEAR_TAIL_SHARE: u64 = 9;

/// Multiplier bound for the near tail.
const NEAR_TAIL_MULT: u64 = 100;

/// Multiplier bound for the far tail — the ~1-in-1000 samples
/// that stretch the distribution by up to four decades.
const FAR_TAIL_MULT: u64 = 10_000;

/// A heavy-tailed tick stream clamped into a histogram's
/// trackable range.
pub struct HeavyTailed {
    rng: SplitMix64,
    max_value: u64,
}

impl HeavyTailed {
    /// Start a stream from `seed`, clamping every value into
    /// `1..=max_value` (typically `Config::max_value`).
    pub fn new(seed: u64, max_value: u64) -> Self {
        HeavyTailed {
            rng: SplitMix64(seed),
            max_value,
        }
    }
}

impl Iterator for HeavyTailed {
    type Item = u64;

    /// One sample: draw the body value, then decide whether
    /// this draw escapes into a tail.
    fn next(&mut self) -> Option<u64> {
        let base = BASE_LO + (self.rng.next() % BASE_SPAN);
        let value = match self.rng.next() % TAIL_ODDS {
            0 => base * (1 + self.rng.next() % FAR_TAIL_MULT),
            1..=NEAR_TAIL_SHARE => base * (1 + self.rng.next() % NEAR_TAIL_MULT),
            _ => base,
        };
        Some(value.clamp(1, self.max_value))
    }
}
