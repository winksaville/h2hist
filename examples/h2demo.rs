//! Demo: record synthetic latencies and print an
//! iiac-perf-style band table — on the library report path.
//!
//! - Builds the shared dev config's histogram, records
//!   [`SAMPLES`] deterministic synthetic ticks, assembles a
//!   [`BandTable`] over the z4/n8 ladder with the exact
//!   rank-split convention, and renders it in the demo's
//!   historical fixed layout.
//! - Everything the table shows comes from the library
//!   modules; only the header lines and the p50/p99 spot
//!   reads live here.

use h2hist::{
    BandAssign, BandLabels, BandTable, Histogram, Ladder, Layout, RankSplit, fmt_commas,
    render_band_table,
};

#[path = "../dev/mod.rs"]
mod dev;

use dev::consts::{BUCKETS, CFG, GROUPING_POWER, MAX_VALUE_POWER, SEED};
use dev::stream::HeavyTailed;

/// Samples the demo records.
const SAMPLES: usize = 1_000_000;

/// The demo's historical ladder depths (z4..n8).
const LADDER: Ladder = match Ladder::new(4, 8) {
    Ok(ladder) => ladder,
    Err(_) => panic!("invalid demo ladder"),
};

/// Band capacity the ladder requires.
const CAP: usize = LADDER.band_count();

/// Quantile read that reports 0 on an empty histogram instead of
/// panicking (never hit here: the demo always records first).
#[allow(clippy::manual_unwrap_or, clippy::manual_unwrap_or_default)]
// OK: project convention forbids unwrap_or*; spelled out as a
// match instead of the suggested `.unwrap_or(0)`.
fn quantile_or_zero(hist: &Histogram<'_, u32>, fraction: f64) -> u64 {
    match hist.quantile(fraction) {
        Some(value) => value,
        None => 0,
    }
}

/// Build the histogram, record samples, and print the demo
/// report.
fn main() -> Result<(), h2hist::Error> {
    let mut storage = vec![0u32; BUCKETS];
    let mut hist = Histogram::new(CFG, &mut storage)?;

    // The assignment convention is the caller's choice; the
    // header names it so the table is self-describing.
    let assigner = RankSplit::new();
    println!("h2demo — h2hist band table ({})", assigner.name());
    println!(
        "config: g={GROUPING_POWER} n={MAX_VALUE_POWER} buckets={BUCKETS} bytes={}",
        BUCKETS * size_of::<u32>()
    );

    for value in HeavyTailed::new(SEED, CFG.max_value()).take(SAMPLES) {
        hist.record(value);
    }
    println!("samples: {}", fmt_commas(hist.total()));
    println!(
        "p50={} p99={} p99.9={} max={}",
        fmt_commas(quantile_or_zero(&hist, 0.50)),
        fmt_commas(quantile_or_zero(&hist, 0.99)),
        fmt_commas(quantile_or_zero(&hist, 0.999)),
        fmt_commas(quantile_or_zero(&hist, 1.0))
    );
    println!();

    let table: BandTable<CAP> = BandTable::build(LADDER, assigner, || hist.buckets())?;
    print!(
        "{}",
        render_band_table(&table, BandLabels::Both, &Layout::DEMO_LEGACY)
    );
    Ok(())
}
