//! Demo: record synthetic latencies and print an iiac-perf-style
//! band table.
//!
//! - Builds a `g=7 n=30` histogram, records 1,000,000
//!   deterministic synthetic samples (seeded splitmix64), then
//!   walks `Histogram::buckets()` once to build quantile-fence
//!   bands plus overall/trimmed mean and stdev.
//! - Every stat is derived from the single bucket pass and its
//!   `cumulative` field, except the closing p50/p99/p99.9/max
//!   line, which uses `quantile()` as intended for spot reads.

use h2hist::Histogram;

#[path = "../dev/mod.rs"]
mod dev;

use dev::consts::{BUCKETS, CFG, GROUPING_POWER, MAX_VALUE_POWER, SEED};
use dev::stream::HeavyTailed;

/// Samples the demo records.
const SAMPLES: usize = 1_000_000;

/// One quantile-fence band's accumulated stats: `first`/`last`
/// bound the recorded values it covers, `count` is its exact
/// rank span, `weighted_sum` accumulates bucket-midpoint mass
/// for the band mean.
#[derive(Debug, Clone, Copy, Default)]
struct BandAcc {
    first: u64,
    last: u64,
    count: u64,
    weighted_sum: f64,
}

/// Label, printable fraction text, and fraction value for each
/// band fence, in ascending-rank order.
const FENCES: [(&str, &str, f64); 19] = [
    ("z4", "0.000_1", 0.000_1),
    ("z3", "0.001", 0.001),
    ("z2", "0.01", 0.01),
    ("p10", "0.10", 0.10),
    ("p20", "0.20", 0.20),
    ("p30", "0.30", 0.30),
    ("p40", "0.40", 0.40),
    ("p50", "0.50", 0.50),
    ("p60", "0.60", 0.60),
    ("p70", "0.70", 0.70),
    ("p80", "0.80", 0.80),
    ("p90", "0.90", 0.90),
    ("n2", "0.99", 0.99),
    ("n3", "0.999", 0.999),
    ("n4", "0.999_9", 0.999_9),
    ("n5", "0.999_99", 0.999_99),
    ("n6", "0.999_999", 0.999_999),
    ("n7", "0.999_999_9", 0.999_999_9),
    ("n8", "0.999_999_99", 0.999_999_99),
];

/// Insert thousands-separator commas into a non-negative
/// integer's decimal text.
fn commas(n: u64) -> String {
    let digits = n.to_string();
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

/// Format a non-negative mean as comma-grouped integer part plus
/// one decimal digit.
fn format_mean(x: f64) -> String {
    let rounded = (x * 10.0).round() / 10.0;
    let int_part = rounded.trunc() as u64;
    let frac_part = ((rounded - int_part as f64) * 10.0).round() as u64;
    format!("{}.{}", commas(int_part), frac_part)
}

/// Quantile read that reports 0 on an empty histogram instead of
/// panicking (never hit here: the demo always records first).
#[allow(clippy::manual_unwrap_or, clippy::manual_unwrap_or_default)]
// OK: project convention forbids unwrap_or*; spelled out as a
// match instead of the suggested `.unwrap_or(0)`.
fn quantile_or_zero(h: &Histogram<'_, u32>, q: f64) -> u64 {
    match h.quantile(q) {
        Some(v) => v,
        None => 0,
    }
}

/// Rank boundaries (1-based, inclusive) for every fence: fence
/// `k` closes at `floor(fraction_k * total)`, clamped to
/// `total`. The last fence with any rank span is then extended
/// to `total` so the printed table's final band absorbs any
/// leftover ranks that rounding left uncovered.
fn rank_boundaries(total: u64) -> [u64; FENCES.len()] {
    let mut boundary = [0u64; FENCES.len()];
    for (i, &(_, _, fraction)) in FENCES.iter().enumerate() {
        boundary[i] = ((fraction * total as f64).floor() as u64).min(total);
    }
    let mut last_nonzero = None;
    let mut prev = 0u64;
    for (i, &b) in boundary.iter().enumerate() {
        if b > prev {
            last_nonzero = Some(i);
        }
        prev = b;
    }
    if let Some(j) = last_nonzero {
        boundary[j] = total;
    }
    boundary
}

/// Record [`SAMPLES`] deterministic synthetic latency ticks
/// into `h`.
fn record_samples(h: &mut Histogram<'_, u32>) {
    for v in HeavyTailed::new(SEED, h.config().max_value()).take(SAMPLES) {
        h.record(v);
    }
}

/// One pass over `h.buckets()`: distributes every recorded rank
/// into its quantile-fence band, and accumulates weighted
/// sum/sum-of-squares both over all ranks and over ranks up to
/// `cap` (the un-adjusted 0.99 fence, for the trimmed summary).
fn build_bands(
    h: &Histogram<'_, u32>,
    boundary: &[u64; FENCES.len()],
    cap: u64,
) -> (Vec<BandAcc>, f64, f64, f64, f64) {
    let mut bands = vec![BandAcc::default(); FENCES.len()];
    let mut prev_cumulative = 0u64;
    let mut bidx = 0usize;
    let mut sum_all = 0f64;
    let mut sumsq_all = 0f64;
    let mut sum_capped = 0f64;
    let mut sumsq_capped = 0f64;

    for b in h.buckets() {
        if b.count == 0 {
            prev_cumulative = b.cumulative;
            continue;
        }
        let start = prev_cumulative + 1;
        let end = b.cumulative;
        let mid = (b.low as f64 + b.high as f64) / 2.0;
        let count_f = b.count as f64;
        sum_all += count_f * mid;
        sumsq_all += count_f * mid * mid;

        if start <= cap {
            let seg_end = end.min(cap);
            let seg_n = (seg_end - start + 1) as f64;
            sum_capped += seg_n * mid;
            sumsq_capped += seg_n * mid * mid;
        }

        // A bucket's rank span can cross several band fences (or
        // a band can span several buckets), so walk both in
        // lockstep rather than assuming a 1:1 match.
        let mut seg_start = start;
        while seg_start <= end && bidx < FENCES.len() {
            while bidx < FENCES.len() && boundary[bidx] < seg_start {
                bidx += 1;
            }
            let Some(band_end) = boundary.get(bidx).copied() else {
                break;
            };
            let seg_end = end.min(band_end);
            if seg_end < seg_start {
                break;
            }
            let seg_n = seg_end - seg_start + 1;
            if let Some(band) = bands.get_mut(bidx) {
                if band.count == 0 {
                    band.first = b.low;
                }
                band.last = b.high;
                band.count += seg_n;
                band.weighted_sum += seg_n as f64 * mid;
            }
            if seg_end == band_end {
                bidx += 1;
            }
            seg_start = seg_end + 1;
        }

        prev_cumulative = b.cumulative;
    }

    (bands, sum_all, sumsq_all, sum_capped, sumsq_capped)
}

/// Print the band table plus the overall/trimmed summary rows.
fn print_table(h: &Histogram<'_, u32>) {
    let total = h.total();
    let boundary = rank_boundaries(total);
    let cap = ((0.99 * total as f64).floor() as u64).min(total);
    let (bands, sum_all, sumsq_all, sum_capped, sumsq_capped) = build_bands(h, &boundary, cap);

    println!(
        "{:<4} {:<13} {:>14} {:>14} {:>14} {:>12} {:>14}",
        "", "", "first", "last", "range", "count", "mean"
    );
    for (i, &(label, fraction_text, _)) in FENCES.iter().enumerate() {
        let Some(band) = bands.get(i) else { continue };
        if band.count == 0 {
            continue;
        }
        let mean = band.weighted_sum / band.count as f64;
        let range = band.last - band.first;
        println!(
            "{:<4} {:<13} {:>14} {:>14} {:>14} {:>12} {:>14}",
            label,
            fraction_text,
            commas(band.first),
            commas(band.last),
            commas(range),
            commas(band.count),
            format_mean(mean)
        );
    }

    let total_f = total as f64;
    let mean_all = sum_all / total_f;
    let stdev_all = (sumsq_all / total_f - mean_all * mean_all).max(0.0).sqrt();
    let cap_f = cap as f64;
    let mean_capped = sum_capped / cap_f;
    let stdev_capped = (sumsq_capped / cap_f - mean_capped * mean_capped)
        .max(0.0)
        .sqrt();

    println!();
    println!("{:<18} {:>14}", "mean", format_mean(mean_all));
    println!("{:<18} {:>14}", "stdev", format_mean(stdev_all));
    println!("{:<18} {:>14}", "mean z4..n2", format_mean(mean_capped));
    println!("{:<18} {:>14}", "stdev z4..n2", format_mean(stdev_capped));
}

/// Build the histogram, record samples, and print the demo
/// report.
fn main() -> Result<(), h2hist::Error> {
    let mut storage = vec![0u32; BUCKETS];
    let mut h = Histogram::new(CFG, &mut storage)?;

    println!("h2demo — h2hist band table");
    println!(
        "config: g={GROUPING_POWER} n={MAX_VALUE_POWER} buckets={BUCKETS} bytes={}",
        BUCKETS * size_of::<u32>()
    );

    record_samples(&mut h);
    println!("samples: {}", commas(h.total()));
    println!(
        "p50={} p99={} p99.9={} max={}",
        commas(quantile_or_zero(&h, 0.50)),
        commas(quantile_or_zero(&h, 0.99)),
        commas(quantile_or_zero(&h, 0.999)),
        commas(quantile_or_zero(&h, 1.0))
    );
    println!();

    print_table(&h);
    Ok(())
}
