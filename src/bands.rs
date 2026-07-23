//! Band-boundary ladder for report tables.
//!
//! - The z/p/n ladder iiac-perf's reports use: familiar
//!   deciles in the body, nines/zeros notation in the tails —
//!   `zK`/`nK` mark the boundary with a fraction `10^-K` of
//!   samples below (z) or above (n), so n2 ≡ p99, z2 ≡ p1.
//! - Pure data and math, `no_std`, no-alloc: fences are
//!   integer rationals (`num/den`), so a rank boundary is
//!   `total * num / den` in u128 — exact, no `floor`/`powi`.
//!   A boundary is the wire-friendly artifact; rendering it
//!   as text (labels, tables) is the render module's job and
//!   lands with it (0.1.3-6).
//! - `Ladder` generates the boundary sequence from its two
//!   tail depths; deepening a tail is a one-argument change.

use crate::config::Error;

/// `10^exp`, saturating at `u64::MAX` (exp ≤ 19 is exact;
/// [`Ladder::new`] rejects deeper tails).
const fn pow10_sat(exp: u8) -> u64 {
    let mut result = 1u64;
    let mut i = 0;
    while i < exp {
        result = result.saturating_mul(10);
        i += 1;
    }
    result
}

/// One boundary of the band ladder, named by its CDF position.
///
/// - `Min` / `Max` — the ladder ends (CDF 0 and 1).
/// - `Z(k)` — fast tail: a fraction `10^-k` of samples at or
///   below it.
/// - `P(d)` — decile `d` in `1..=9`: a fraction `d/10` at or
///   below it.
/// - `N(k)` — slow tail: a fraction `10^-k` of samples above
///   it (n2 ≡ p99, n3 ≡ p99.9, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boundary {
    /// CDF 0 — below every sample.
    Min,
    /// Fast tail, fraction `10^-k` at or below.
    Z(u8),
    /// Decile `d` in `1..=9`, fraction `d/10` at or below.
    P(u8),
    /// Slow tail, fraction `10^-k` above.
    N(u8),
    /// CDF 1 — at or above every sample.
    Max,
}

impl Boundary {
    /// The boundary's CDF fraction as an exact rational
    /// `(num, den)`.
    pub const fn fraction(&self) -> (u64, u64) {
        match *self {
            Boundary::Min => (0, 1),
            Boundary::Z(k) => (1, pow10_sat(k)),
            Boundary::P(d) => (d as u64, 10),
            Boundary::N(k) => {
                let den = pow10_sat(k);
                (den - 1, den)
            }
            Boundary::Max => (1, 1),
        }
    }

    /// The boundary's rank in a run of `total` samples:
    /// `floor(total * num / den)`, computed exactly in u128
    /// (no float `floor`).
    pub const fn rank(&self, total: u64) -> u64 {
        let (num, den) = self.fraction();
        ((total as u128 * num as u128) / den as u128) as u64
    }

    /// The boundary's CDF fraction as an f64, for consumers
    /// working in mid-rank space.
    pub fn pct(&self) -> f64 {
        let (num, den) = self.fraction();
        num as f64 / den as f64
    }
}

/// The boundary ladder, generated from its two tail depths:
/// `min, z{z_depth}..z2, p10..p90, n2..n{n_depth}, max`.
///
/// - `n_depth` is typically deeper than `z_depth`: a latency
///   distribution is floored below (nothing beats the fast
///   path) and open above.
/// - Positions are generated on demand ([`get`](Ladder::get) /
///   [`iter`](Ladder::iter)) — no allocation, no stored table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ladder {
    z_depth: u8,
    n_depth: u8,
}

impl Ladder {
    /// Build a ladder; depths outside `2..=19` are rejected
    /// ([`Error::BandDepth`]) — below 2 the tail vanishes into
    /// the deciles, above 19 the `10^depth` fence math leaves
    /// u64.
    pub const fn new(z_depth: u8, n_depth: u8) -> Result<Ladder, Error> {
        if z_depth < 2 || z_depth > 19 || n_depth < 2 || n_depth > 19 {
            return Err(Error::BandDepth);
        }
        Ok(Ladder { z_depth, n_depth })
    }

    /// Fast-tail depth: the lowest tail boundary is
    /// `z{z_depth}`.
    pub const fn z_depth(&self) -> u8 {
        self.z_depth
    }

    /// Slow-tail depth: the highest tail boundary is
    /// `n{n_depth}`.
    pub const fn n_depth(&self) -> u8 {
        self.n_depth
    }

    /// Number of boundaries: min + (z_depth−1) z's + 9
    /// deciles + (n_depth−1) n's + max.
    pub const fn len(&self) -> usize {
        (self.z_depth as usize - 1) + (self.n_depth as usize - 1) + 11
    }

    /// Never true — a valid ladder always has min, deciles,
    /// and max ([`len`](Ladder::len) ≥ 13); present because
    /// `len` conventionally pairs with it.
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// The boundary at `index` (ascending CDF), `None` past
    /// the end.
    pub const fn get(&self, index: usize) -> Option<Boundary> {
        if index == 0 {
            return Some(Boundary::Min);
        }
        let mut idx = index - 1;
        let z_count = self.z_depth as usize - 1;
        if idx < z_count {
            return Some(Boundary::Z(self.z_depth - idx as u8));
        }
        idx -= z_count;
        if idx < 9 {
            return Some(Boundary::P(idx as u8 + 1));
        }
        idx -= 9;
        let n_count = self.n_depth as usize - 1;
        if idx < n_count {
            return Some(Boundary::N(idx as u8 + 2));
        }
        if idx == n_count {
            return Some(Boundary::Max);
        }
        None
    }

    /// Iterate the boundaries lowest-CDF first.
    pub const fn iter(&self) -> LadderIter {
        LadderIter {
            ladder: *self,
            index: 0,
        }
    }
}

/// Iterator over a [`Ladder`]'s boundaries, ascending CDF.
#[derive(Debug)]
pub struct LadderIter {
    ladder: Ladder,
    index: usize,
}

impl Iterator for LadderIter {
    type Item = Boundary;

    fn next(&mut self) -> Option<Boundary> {
        let boundary = self.ladder.get(self.index)?;
        self.index += 1;
        Some(boundary)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let rest = self.ladder.len() - self.index;
        (rest, Some(rest))
    }
}

impl ExactSizeIterator for LadderIter {}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // OK: tests panic on setup failure by design
mod tests {
    use super::*;
    use Boundary::{Max, Min, N, P, Z};

    /// The z4/n10 ladder must match iiac-perf's documented
    /// boundary sequence.
    #[test]
    fn ladder_matches_iiac_perf() {
        let ladder = Ladder::new(4, 10).unwrap();
        let boundaries: Vec<Boundary> = ladder.iter().collect();
        assert_eq!(
            boundaries,
            [
                Min,
                Z(4),
                Z(3),
                Z(2),
                P(1),
                P(2),
                P(3),
                P(4),
                P(5),
                P(6),
                P(7),
                P(8),
                P(9),
                N(2),
                N(3),
                N(4),
                N(5),
                N(6),
                N(7),
                N(8),
                N(9),
                N(10),
                Max,
            ]
        );
        assert_eq!(ladder.len(), 23);
        assert_eq!(ladder.iter().len(), 23);
        assert_eq!(ladder.get(23), None);
    }

    /// Fractions are exact: endpoints 0 and 1, interior
    /// strictly increasing (compared as cross-multiplied
    /// rationals, no float slack).
    #[test]
    fn fractions_exact_and_increasing() {
        let ladder = Ladder::new(4, 10).unwrap();
        assert_eq!(ladder.get(0).unwrap().fraction(), (0, 1));
        assert_eq!(ladder.get(ladder.len() - 1).unwrap().fraction(), (1, 1));
        let boundaries: Vec<Boundary> = ladder.iter().collect();
        for pair in boundaries.windows(2) {
            let (num_lo, den_lo) = pair[0].fraction();
            let (num_hi, den_hi) = pair[1].fraction();
            assert!(
                (num_lo as u128 * den_hi as u128) < (num_hi as u128 * den_lo as u128),
                "{:?} !< {:?}",
                pair[0],
                pair[1]
            );
        }
        // Spot values.
        assert_eq!(Z(4).fraction(), (1, 10_000));
        assert_eq!(P(5).fraction(), (5, 10));
        assert_eq!(N(2).fraction(), (99, 100));
    }

    /// Rank boundaries come out exact where float floor math
    /// is only approximate.
    #[test]
    fn ranks_exact() {
        assert_eq!(Min.rank(1_000_000), 0);
        assert_eq!(Max.rank(1_000_000), 1_000_000);
        assert_eq!(Z(4).rank(1_000_000), 100);
        assert_eq!(P(5).rank(1_000_001), 500_000);
        assert_eq!(N(2).rank(1_000), 990);
        // Huge totals stay exact via the u128 product.
        assert_eq!(P(9).rank(u64::MAX), ((u64::MAX as u128 * 9) / 10) as u64);
        // n2 pct lands within a ulp of 0.99 (the trim anchor).
        assert!((N(2).pct() - 0.99).abs() < 1e-12);
    }

    /// Depths outside 2..=19 are rejected.
    #[test]
    fn depth_validation() {
        assert!(Ladder::new(2, 2).is_ok());
        assert!(Ladder::new(4, 19).is_ok());
        assert!(matches!(Ladder::new(1, 10), Err(Error::BandDepth)));
        assert!(matches!(Ladder::new(4, 1), Err(Error::BandDepth)));
        assert!(matches!(Ladder::new(20, 10), Err(Error::BandDepth)));
        assert!(matches!(Ladder::new(4, 20), Err(Error::BandDepth)));
    }

    /// The demo's z4/n8 depths also generate correctly (the
    /// -7 rewrite records at these depths first): the demo's
    /// 19 fences plus min and max.
    #[test]
    fn demo_depths() {
        let ladder = Ladder::new(4, 8).unwrap();
        assert_eq!(ladder.len(), 21);
        assert_eq!(ladder.get(0), Some(Min));
        assert_eq!(ladder.get(ladder.len() - 2), Some(N(8)));
        assert_eq!(ladder.get(ladder.len() - 1), Some(Max));
    }
}
