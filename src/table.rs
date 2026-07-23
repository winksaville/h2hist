//! The assembled band table — the structure a report renders.
//!
//! - `BandTable` is the ship-structs-to-a-service artifact:
//!   per-band accumulators plus the overall and tail-trimmed
//!   summary stats, all numbers, no text. Rendering it is the
//!   render module's job (0.1.3-6).
//! - Fixed-capacity (`CAP` bands inline), `no_std`, no-alloc;
//!   `CAP` comes from a `const Ladder` via
//!   [`Ladder::band_count`], checked at build.
//! - Built from a re-creatable bucket stream (several cheap
//!   passes: total, assignment, then the two-pass stats).

use crate::analysis::Bucket;
use crate::bands::{Band, BandAssign, Boundary, Ladder};
use crate::config::Error;
use crate::stats::Stats;

/// A ladder's bands plus summary stats, assembled from one
/// bucket stream.
///
/// - `bands()` — one [`Band`] per ladder band, ascending.
/// - `overall()` / `trimmed()` — [`Stats`] over all ranks and
///   over `(0, rank(n2)]` (everything below the n2 ≡ p99 tail
///   cut, the standard trim anchor).
/// - `trim_range()` — the populated extent below the cut, for
///   the `mean z4..n2`-style row label.
#[derive(Debug, Clone, Copy)]
pub struct BandTable<const CAP: usize> {
    ladder: Ladder,
    total: u64,
    band_count: usize,
    bands: [Band; CAP],
    overall: Stats,
    trimmed: Stats,
}

impl<const CAP: usize> BandTable<CAP> {
    /// Assemble a table: distribute the stream into bands with
    /// `assigner`, then derive the overall and trimmed stats.
    ///
    /// - `CAP` must be at least `ladder.band_count()`
    ///   ([`Error::TableCapacity`] otherwise); compute it from
    ///   a `const Ladder`.
    /// - `buckets` is called for each pass, so pass a cheap
    ///   constructor — e.g. `|| hist.buckets()`.
    pub fn build<Assigner, Iter, Make>(
        ladder: Ladder,
        mut assigner: Assigner,
        buckets: Make,
    ) -> Result<BandTable<CAP>, Error>
    where
        Assigner: BandAssign,
        Make: Fn() -> Iter,
        Iter: Iterator<Item = Bucket>,
    {
        let band_count = ladder.band_count();
        if band_count > CAP {
            return Err(Error::TableCapacity);
        }

        // Total: the last bucket's cumulative count.
        let mut total = 0u64;
        for bucket in buckets() {
            total = bucket.cumulative;
        }

        // Assignment pass.
        let mut bands = [Band::default(); CAP];
        for bucket in buckets() {
            assigner.assign(&bucket, total, &ladder, &mut bands[..band_count]);
        }

        // Summary stats; the trim window ends at the n2 fence.
        let overall = Stats::from_buckets(&buckets);
        let trimmed = Stats::from_window(&buckets, 0, Boundary::N(2).rank(total));

        Ok(BandTable {
            ladder,
            total,
            band_count,
            bands,
            overall,
            trimmed,
        })
    }

    /// The ladder this table was assembled over.
    pub const fn ladder(&self) -> Ladder {
        self.ladder
    }

    /// Total recorded occurrences.
    pub const fn total(&self) -> u64 {
        self.total
    }

    /// The assembled bands, ascending — `ladder().band_count()`
    /// entries.
    pub fn bands(&self) -> &[Band] {
        &self.bands[..self.band_count]
    }

    /// Stats over every recorded rank.
    pub const fn overall(&self) -> Stats {
        self.overall
    }

    /// Stats over `(0, rank(n2)]` — everything below the
    /// n2 ≡ p99 tail cut.
    pub const fn trimmed(&self) -> Stats {
        self.trimmed
    }

    /// Upper boundaries of the first and last populated bands
    /// below the n2 tail cut — the `z4..n2`-style trim label
    /// extent. `None` when nothing below the cut is populated.
    pub fn trim_range(&self) -> Option<(Boundary, Boundary)> {
        // N(2) sits at ladder index z_depth + 9; bands below
        // the cut are those capped by boundaries 1..=that.
        let trim_bands = self.ladder.z_depth() as usize + 9;
        let mut first = None;
        let mut last = None;
        for (index, band) in self.bands().iter().enumerate().take(trim_bands) {
            if band.count > 0 {
                if first.is_none() {
                    first = Some(index);
                }
                last = Some(index);
            }
        }
        match (first, last) {
            (Some(lo), Some(hi)) => {
                let lo_boundary = self.ladder.get(lo + 1)?;
                let hi_boundary = self.ladder.get(hi + 1)?;
                Some((lo_boundary, hi_boundary))
            }
            _ => None,
        }
    }
}

impl Ladder {
    /// Number of bands between the boundaries,
    /// [`len`](Ladder::len)` - 1` — the `CAP` a
    /// [`BandTable`] over this ladder needs.
    pub const fn band_count(&self) -> usize {
        self.len() - 1
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // OK: tests panic on setup failure by design
mod tests {
    use super::*;
    use crate::bands::{MidRank, RankSplit};
    use crate::{Config, Histogram};

    const LADDER: Ladder = match Ladder::new(2, 2) {
        Ok(ladder) => ladder,
        Err(_) => panic!("invalid test ladder"),
    };
    const CAP: usize = LADDER.band_count();

    /// Build a config at compile time; invalid powers are a
    /// test-authoring error, so they fail the build rather
    /// than the run.
    const fn cfg(grouping_power: u8, max_value_power: u8) -> Config {
        match Config::new(grouping_power, max_value_power) {
            Ok(config) => config,
            Err(_) => panic!("invalid test config"),
        }
    }

    /// A table over the -3 full-pass histogram matches a
    /// manual assignment pass and the Stats windows.
    #[test]
    fn table_matches_manual_passes() {
        const CFG: Config = cfg(2, 8);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        for value in 0..10u64 {
            hist.record(value);
        }

        let table: BandTable<CAP> =
            BandTable::build(LADDER, RankSplit::new(), || hist.buckets()).unwrap();
        assert_eq!(table.total(), 10);
        assert_eq!(table.bands().len(), 12);

        let mut manual = [Band::default(); CAP];
        let mut splitter = RankSplit::new();
        for bucket in hist.buckets() {
            splitter.assign(&bucket, 10, &LADDER, &mut manual);
        }
        assert_eq!(table.bands(), &manual[..]);

        let overall = Stats::from_buckets(|| hist.buckets());
        assert_eq!(table.overall(), overall);
        // n2 rank of 10 samples: floor(10 * 99/100) = 9.
        let trimmed = Stats::from_window(|| hist.buckets(), 0, 9);
        assert_eq!(table.trimmed(), trimmed);
        assert_eq!(table.trimmed().count, 9);
    }

    /// The trim range names the populated extent below the n2
    /// cut by upper boundary.
    #[test]
    fn trim_range_extent() {
        const CFG: Config = cfg(2, 8);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        for value in 0..10u64 {
            hist.record(value);
        }
        let table: BandTable<CAP> =
            BandTable::build(LADDER, RankSplit::new(), || hist.buckets()).unwrap();
        // Ranks 1..9 populate the p10..p90 bands; rank 10 goes
        // to max (past the cut) and the z2 span is empty.
        assert_eq!(table.trim_range(), Some((Boundary::P(1), Boundary::P(9))));
    }

    /// MidRank assembly conserves the total.
    #[test]
    fn mid_rank_build_conserves_total() {
        const CFG: Config = cfg(2, 8);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        for value in 0..100u64 {
            hist.record(value % 20);
        }
        let table: BandTable<CAP> =
            BandTable::build(LADDER, MidRank::new(), || hist.buckets()).unwrap();
        let band_total: u64 = table.bands().iter().map(|band| band.count).sum();
        assert_eq!(band_total, table.total());
    }

    /// Capacity below the ladder's band count is rejected.
    #[test]
    fn capacity_checked() {
        const CFG: Config = cfg(2, 8);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let hist = Histogram::new(CFG, &mut counts).unwrap();
        let short: Result<BandTable<{ CAP - 1 }>, Error> =
            BandTable::build(LADDER, RankSplit::new(), || hist.buckets());
        assert!(matches!(short, Err(Error::TableCapacity)));
    }

    /// An empty histogram assembles to a zeroed table.
    #[test]
    fn empty_histogram() {
        const CFG: Config = cfg(2, 8);
        const BUCKETS: usize = CFG.total_buckets();
        let mut counts = [0u32; BUCKETS];
        let hist = Histogram::new(CFG, &mut counts).unwrap();
        let table: BandTable<CAP> =
            BandTable::build(LADDER, RankSplit::new(), || hist.buckets()).unwrap();
        assert_eq!(table.total(), 0);
        assert!(table.bands().iter().all(|band| band.count == 0));
        assert_eq!(table.overall(), Stats::default());
        assert_eq!(table.trimmed(), Stats::default());
        assert_eq!(table.trim_range(), None);
    }
}
