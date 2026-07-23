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
//! - `BandAssign` distributes a bucket stream into the
//!   ladder's bands; two conventions ship ([`RankSplit`] and
//!   [`MidRank`]) because the demo and iiac-perf legitimately
//!   disagree on where a fence-straddling bucket belongs.

use crate::analysis::Bucket;
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

/// One band's accumulated stats. Band `i` spans the ranks
/// between boundaries `i` and `i+1` of its ladder — labeled by
/// the **upper** boundary, `(lower, upper]`.
///
/// - `first` / `last` — value bounds of the contributing
///   buckets (lowest bucket low, highest bucket high).
/// - `count` — occurrences assigned to this band.
/// - `weighted_sum` — bucket-midpoint mass, the numerator of
///   the band mean.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Band {
    /// Low end of the first contributing bucket's range.
    pub first: u64,
    /// High end of the last contributing bucket's range.
    pub last: u64,
    /// Occurrences assigned to this band.
    pub count: u64,
    /// Bucket-midpoint weighted mass.
    pub weighted_sum: f64,
}

impl Band {
    /// Fold `count` occurrences from a bucket spanning
    /// `low..=high` into this band.
    fn fold(&mut self, low: u64, high: u64, count: u64) {
        if self.count == 0 {
            self.first = low;
        }
        self.last = high;
        self.count += count;
        let mid = (low as f64 + high as f64) / 2.0;
        self.weighted_sum += count as f64 * mid;
    }
}

/// Distribute one bucket's counts into a ladder's bands.
///
/// - Called once per bucket, ascending, over one full pass of
///   a histogram's [`Buckets`](crate::Buckets) iterator; the
///   implementation may keep walk state, so use a fresh value
///   per pass.
/// - `bands.len()` must be `ladder.len() - 1`; band `i` is
///   capped by boundary `i + 1`.
pub trait BandAssign {
    /// Fold `bucket` into `bands`.
    fn assign(&mut self, bucket: &Bucket, total: u64, ladder: &Ladder, bands: &mut [Band]);
}

/// Exact rank-split assignment (the demo's convention).
///
/// A bucket's rank span `(cumulative - count, cumulative]` is
/// split across every fence it crosses, so band counts are
/// exact rank spans — a band's `count` is precisely how many
/// ranks fall inside its fences, at the cost of a bucket's
/// value range landing in several bands.
#[derive(Debug, Default)]
pub struct RankSplit {
    band_index: usize,
}

impl RankSplit {
    /// Fresh walk state for one pass.
    pub fn new() -> RankSplit {
        RankSplit::default()
    }
}

impl BandAssign for RankSplit {
    fn assign(&mut self, bucket: &Bucket, total: u64, ladder: &Ladder, bands: &mut [Band]) {
        if bucket.count == 0 {
            return;
        }
        let start = bucket.cumulative - bucket.count + 1;
        let end = bucket.cumulative;

        // Walk the bucket's rank span and the fences in
        // lockstep: a span can cross several fences, or a band
        // can span several buckets.
        let mut seg_start = start;
        while seg_start <= end && self.band_index < bands.len() {
            // Advance past bands whose end fence is below the
            // segment.
            let Some(upper) = ladder.get(self.band_index + 1) else {
                break;
            };
            let band_end = upper.rank(total);
            if band_end < seg_start {
                self.band_index += 1;
                continue;
            }
            let seg_end = if end < band_end { end } else { band_end };
            if let Some(band) = bands.get_mut(self.band_index) {
                band.fold(bucket.low, bucket.high, seg_end - seg_start + 1);
            }
            if seg_end == band_end {
                self.band_index += 1;
            }
            seg_start = seg_end + 1;
        }
    }
}

/// Whole-bucket mid-rank assignment (iiac-perf's convention).
///
/// Each bucket goes wholly to the band containing its Hazen
/// mid-rank `(cumulative_before + count/2) / total`, bands
/// right-closed `(lower, upper]` — a rank exactly on a fence
/// falls in the band that fence caps. Band counts are
/// approximate when buckets are coarse, but a bucket's values
/// never split across bands.
///
/// The comparison is done in integers (u128 cross-multiply
/// against the fence rational), so it is exact where a float
/// `pct` compare has ulp slack; products saturate as a
/// backstop in regimes far beyond practical totals.
#[derive(Debug, Default)]
pub struct MidRank;

impl MidRank {
    /// Fresh (stateless) assigner for one pass.
    pub fn new() -> MidRank {
        MidRank
    }
}

impl BandAssign for MidRank {
    fn assign(&mut self, bucket: &Bucket, total: u64, ladder: &Ladder, bands: &mut [Band]) {
        if bucket.count == 0 || total == 0 {
            return;
        }
        // Twice the mid-rank numerator: 2*(cum_before) + count.
        let mid2 = (2 * bucket.cumulative as u128) - bucket.count as u128;
        // First band whose upper fence is at or above the
        // mid-rank; the last band is the fallback.
        let mut band_index = bands.len() - 1;
        let mut index = 0;
        while index < bands.len() {
            if let Some(upper) = ladder.get(index + 1) {
                let (num, den) = upper.fraction();
                // mid2 / (2*total) <= num / den, cross-multiplied.
                let lhs = mid2.saturating_mul(den as u128);
                let rhs = (2 * total as u128).saturating_mul(num as u128);
                if lhs <= rhs {
                    band_index = index;
                    break;
                }
            }
            index += 1;
        }
        if let Some(band) = bands.get_mut(band_index) {
            band.fold(bucket.low, bucket.high, bucket.count);
        }
    }
}

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

    /// A bucket whose rank span is the whole run: the two
    /// conventions legitimately disagree — `RankSplit` spreads
    /// the counts across every band's exact rank span,
    /// `MidRank` drops the whole bucket in the p50 band.
    #[test]
    fn conventions_disagree_on_straddling_bucket() {
        let ladder = Ladder::new(2, 2).unwrap(); // 13 bounds, 12 bands
        let bucket = Bucket {
            low: 5,
            high: 5,
            count: 100,
            cumulative: 100,
        };

        let mut split_bands = [Band::default(); 12];
        RankSplit::new().assign(&bucket, 100, &ladder, &mut split_bands);
        let split_counts: Vec<u64> = split_bands.iter().map(|band| band.count).collect();
        // Fences at ranks 1, 10, 20, .. 90, 99, 100.
        assert_eq!(split_counts, [1, 9, 10, 10, 10, 10, 10, 10, 10, 10, 9, 1]);
        assert_eq!(split_bands.iter().map(|band| band.count).sum::<u64>(), 100);

        let mut mid_bands = [Band::default(); 12];
        MidRank::new().assign(&bucket, 100, &ladder, &mut mid_bands);
        let mid_counts: Vec<u64> = mid_bands.iter().map(|band| band.count).collect();
        // Mid-rank 0.5, right-closed: all 100 in the p50 band.
        assert_eq!(mid_counts, [0, 0, 0, 0, 0, 100, 0, 0, 0, 0, 0, 0]);
    }

    /// Mid-rank exactly on a fence lands in the band that
    /// fence caps (right-closed), mirroring iiac-perf's
    /// `band_index` cases.
    #[test]
    fn mid_rank_right_closed() {
        let ladder = Ladder::new(2, 2).unwrap();
        // Band indices: 0 z2, 1 p10, .. 9 p90, 10 n2, 11 max.
        let cases = [
            (50u64, 20u64, 4usize), // mid 0.40 → exactly the p40 fence
            (100, 2, 10),           // mid 0.99 → exactly the n2 fence
            (50, 10, 5),            // mid 0.45 → interior, p50 band
            (100, 100, 5),          // mid 0.50 → exactly the p50 fence
        ];
        for (cumulative, count, expected_band) in cases {
            let bucket = Bucket {
                low: 7,
                high: 7,
                count,
                cumulative,
            };
            let mut bands = [Band::default(); 12];
            MidRank::new().assign(&bucket, 100, &ladder, &mut bands);
            for (index, band) in bands.iter().enumerate() {
                let expected = if index == expected_band { count } else { 0 };
                assert_eq!(
                    band.count, expected,
                    "cum={cumulative} count={count} band={index}"
                );
            }
        }
    }

    /// A full histogram pass: `RankSplit` partitions the total
    /// exactly (fence spans with zero ranks stay empty),
    /// `MidRank` conserves the total; the top value splits
    /// them (max band vs n2 band).
    #[test]
    fn full_pass_over_histogram_buckets() {
        use crate::{Config, Histogram};
        const CFG: Config = match Config::new(2, 8) {
            Ok(config) => config,
            Err(_) => panic!("invalid test config"),
        };
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        for value in 0..10u64 {
            hist.record(value);
        }

        let ladder = Ladder::new(2, 2).unwrap();
        let total = hist.total();

        let mut split_bands = [Band::default(); 12];
        let mut splitter = RankSplit::new();
        for bucket in hist.buckets() {
            splitter.assign(&bucket, total, &ladder, &mut split_bands);
        }
        let split_counts: Vec<u64> = split_bands.iter().map(|band| band.count).collect();
        // total=10: z2 fence rank 0 (empty), deciles rank 1..9,
        // n2 fence rank 9 (empty span), max takes rank 10.
        assert_eq!(split_counts, [0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1]);

        let mut mid_bands = [Band::default(); 12];
        let mut mid = MidRank::new();
        for bucket in hist.buckets() {
            mid.assign(&bucket, total, &ladder, &mut mid_bands);
        }
        assert_eq!(mid_bands.iter().map(|band| band.count).sum::<u64>(), 10);
        // Values 8 and 9 share one bucket (the exact region
        // ends at 7); its mid-rank is exactly 0.9 → the whole
        // pair lands in the p90 band, while RankSplit put
        // rank 10 in the max band — the conventions split the
        // top of this run differently.
        assert_eq!(mid_bands[9].count, 2);
        assert_eq!(mid_bands[10].count, 0);
        assert_eq!(mid_bands[11].count, 0);
    }

    /// `Band::fold` semantics via assignment: first/last track
    /// the contributing buckets' bounds, weighted_sum their
    /// midpoint mass; empty buckets change nothing.
    #[test]
    fn band_fold_bounds_and_mass() {
        let ladder = Ladder::new(2, 2).unwrap();
        let mut bands = [Band::default(); 12];
        let mut mid = MidRank::new();
        // Two buckets, both mid-rank interior to the p50 band.
        let first = Bucket {
            low: 10,
            high: 11,
            count: 3,
            cumulative: 44,
        };
        let second = Bucket {
            low: 12,
            high: 13,
            count: 4,
            cumulative: 48,
        };
        let empty = Bucket {
            low: 14,
            high: 15,
            count: 0,
            cumulative: 48,
        };
        mid.assign(&first, 100, &ladder, &mut bands);
        mid.assign(&second, 100, &ladder, &mut bands);
        mid.assign(&empty, 100, &ladder, &mut bands);
        let band = &bands[5];
        assert_eq!(band.first, 10);
        assert_eq!(band.last, 13);
        assert_eq!(band.count, 7);
        let expected = 3.0 * 10.5 + 4.0 * 12.5;
        assert!((band.weighted_sum - expected).abs() < 1e-9);
    }
}
