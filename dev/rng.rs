//! Deterministic 64-bit PRNG.
//!
//! - splitmix64: a seed, a fixed odd increment, and two
//!   xor-shift-multiply rounds. Chosen because it is a handful
//!   of lines and needs no dependency, and because a fixed
//!   seed makes every consumer's stream reproducible.
//! - Not cryptographic and not a general-purpose generator —
//!   it exists to make test, bench, and demo runs comparable.

/// A splitmix64 generator; the field is its running state, so
/// `SplitMix64(seed)` starts a stream.
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    /// Next raw u64.
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut mixed = self.0;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^ (mixed >> 31)
    }
}
