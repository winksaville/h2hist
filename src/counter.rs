//! Bucket-counter abstraction.
//!
//! - `Counter` lets counts storage be u8/u16/u32/u64, trading
//!   footprint for headroom; u32 is the crate default.
//! - All arithmetic saturates: a full counter pins at MAX
//!   rather than wrapping or panicking.

/// A bucket count: unsigned, copyable, saturating.
///
/// - `to_u64` widens losslessly.
/// - `from_u64_sat` narrows, pinning at `Self`'s MAX.
/// - `sat_add` is the record-path increment.
pub trait Counter: Copy + Default {
    /// Widen to u64 (lossless).
    fn to_u64(self) -> u64;

    /// Narrow from u64, saturating at this type's MAX.
    fn from_u64_sat(value: u64) -> Self;

    /// Saturating add of a u64 delta.
    #[inline]
    fn sat_add(self, by: u64) -> Self {
        Self::from_u64_sat(self.to_u64().saturating_add(by))
    }
}

/// Implement `Counter` for an unsigned primitive.
macro_rules! impl_counter {
    ($($ty:ty),*) => {$(
        impl Counter for $ty {
            #[inline]
            fn to_u64(self) -> u64 {
                self as u64
            }

            #[inline]
            fn from_u64_sat(value: u64) -> Self {
                if value > <$ty>::MAX as u64 {
                    <$ty>::MAX
                } else {
                    value as $ty
                }
            }
        }
    )*};
}

impl_counter!(u8, u16, u32, u64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widen_narrow() {
        assert_eq!(300u64, <u16 as Counter>::from_u64_sat(300).to_u64());
        assert_eq!(u8::MAX, <u8 as Counter>::from_u64_sat(300));
        assert_eq!(u64::MAX, <u64 as Counter>::from_u64_sat(u64::MAX));
    }

    #[test]
    fn saturating_add() {
        assert_eq!(u8::MAX, 250u8.sat_add(10));
        assert_eq!(255u8, u8::MAX.sat_add(1));
        assert_eq!(10u32, 7u32.sat_add(3));
        assert_eq!(u64::MAX, (u64::MAX - 1).sat_add(5));
    }
}
