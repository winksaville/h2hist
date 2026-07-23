//! Rendering: turn report structures into text. `std`-only.
//!
//! - The render side of the device/service split: a device
//!   ships the `no_std` structures (`bands.rs`, `table.rs`);
//!   the service — always a host with `std` — turns them into
//!   text. Gating the whole module on `std` states that
//!   architecture at the crate boundary and keeps the code on
//!   plain `String`/`format!`.
//! - Boundary labels live here, not on the data types: a
//!   label is presentation, and a device that ships structs
//!   never links this module.
//! - `render_band_table` reproduces the band-table shape the
//!   demo and iiac-perf print: header, one row per populated
//!   band (first/last/range/count/mean), then overall and
//!   tail-trimmed mean/stdev rows.

use crate::bands::Boundary;
use crate::numfmt::{fmt_commas, fmt_commas_f64};
use crate::table::BandTable;

/// Band-label style for report rows.
///
/// - `Zpn` — nines/zeros + decile names (`z3`, `p50`, `n4`).
/// - `Frac` — literal boundary fractions with `_` grouping
///   (`0.001`, `0.50`, `0.999_9`).
/// - `Both` — zpn and fraction side by side; the juxtaposition
///   teaches the zpn vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandLabels {
    /// nines/zeros + decile names.
    Zpn,
    /// Literal fractions.
    Frac,
    /// zpn and fraction side by side.
    Both,
}

impl BandLabels {
    /// Lowercase name for CLI flags and report-header
    /// metadata.
    pub const fn as_str(self) -> &'static str {
        match self {
            BandLabels::Zpn => "zpn",
            BandLabels::Frac => "frac",
            BandLabels::Both => "both",
        }
    }
}

/// The nines/zeros / decile name (`z3`, `p50`, `n4`, `min`).
pub fn zpn_label(boundary: Boundary) -> String {
    match boundary {
        Boundary::Min => "min".to_string(),
        Boundary::Max => "max".to_string(),
        Boundary::Z(k) => format!("z{k}"),
        Boundary::P(d) => format!("p{d}0"),
        Boundary::N(k) => format!("n{k}"),
    }
}

/// The literal-fraction name with `_` grouping (`0.001`,
/// `0.50`, `0.999_9`, `min`).
pub fn frac_label(boundary: Boundary) -> String {
    match boundary {
        Boundary::Min => "min".to_string(),
        Boundary::Max => "max".to_string(),
        Boundary::P(d) => format!("0.{d}0"),
        Boundary::Z(k) => grouped(k, '0', true),
        Boundary::N(k) => grouped(k, '9', false),
    }
}

/// The single-cell row label in the given style.
///
/// - `Both` pads the zpn name to its 3-char max so the
///   fraction column aligns (`n4  0.999_9`); `min`/`max`
///   carry no fraction, so `Both` prints them bare.
pub fn boundary_label(boundary: Boundary, style: BandLabels) -> String {
    match style {
        BandLabels::Zpn => zpn_label(boundary),
        BandLabels::Frac => frac_label(boundary),
        BandLabels::Both => match boundary {
            Boundary::Min | Boundary::Max => zpn_label(boundary),
            _ => format!("{:<3} {}", zpn_label(boundary), frac_label(boundary)),
        },
    }
}

/// The bare style-name used in a trimmed-stat range label —
/// `Both` reuses the zpn name (`z4`, not the padded pair) so
/// it reads cleanly in a `z4..n2` range.
pub fn trim_name(boundary: Boundary, style: BandLabels) -> String {
    match style {
        BandLabels::Frac => frac_label(boundary),
        BandLabels::Zpn | BandLabels::Both => zpn_label(boundary),
    }
}

/// `0.` then `digit_count` digits of `fill`, `_`-grouped in
/// threes; `trailing_one` replaces the last digit with `1`
/// (the zK fractions: `0.000_1`).
fn grouped(digit_count: u8, fill: char, trailing_one: bool) -> String {
    let mut out = String::from("0.");
    for index in 0..digit_count {
        if index > 0 && index.is_multiple_of(3) {
            out.push('_');
        }
        let last = index + 1 == digit_count;
        out.push(if trailing_one && last { '1' } else { fill });
    }
    out
}

/// Column widths for [`render_band_table`], in characters.
///
/// - `zpn_width` — the (first) label cell; single-cell styles
///   put their whole label here.
/// - `frac_width` — the second label cell; used by
///   [`BandLabels::Both`], 0 otherwise.
/// - `value_width` — first / last / range / mean columns.
/// - `count_width` — the count column.
/// - `decimals` — fractional digits on the mean/stdev cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// First (zpn) label cell width.
    pub zpn_width: usize,
    /// Second (fraction) label cell width; 0 disables it.
    pub frac_width: usize,
    /// Width of the first/last/range/mean columns.
    pub value_width: usize,
    /// Width of the count column.
    pub count_width: usize,
    /// Fractional digits on mean/stdev cells.
    pub decimals: usize,
}

impl Layout {
    /// The demo's historical fixed layout
    /// (`{:<4} {:<13} {:>14} … {:>12} {:>14}`, one decimal).
    pub const DEMO_LEGACY: Layout = Layout {
        zpn_width: 4,
        frac_width: 13,
        value_width: 14,
        count_width: 12,
        decimals: 1,
    };

    /// Measure a snug layout for `table` in `style`: widest
    /// cell per column, headers and summary rows included.
    pub fn measure<const CAP: usize>(
        table: &BandTable<CAP>,
        style: BandLabels,
        decimals: usize,
    ) -> Layout {
        let two_cell = matches!(style, BandLabels::Both);
        let mut zpn_width = 0usize;
        let mut frac_width = 0usize;
        let mut value_width = "first".len(); // widest header
        let mut count_width = "count".len();

        for (index, band) in table.bands().iter().enumerate() {
            if band.count == 0 {
                continue;
            }
            let Some(upper) = table.ladder().get(index + 1) else {
                continue;
            };
            let zpn_len = if two_cell {
                zpn_label(upper).len()
            } else {
                boundary_label(upper, style).len()
            };
            zpn_width = zpn_width.max(zpn_len);
            if two_cell && !matches!(upper, Boundary::Min | Boundary::Max) {
                frac_width = frac_width.max(frac_label(upper).len());
            }
            let mean = band.weighted_sum / band.count as f64;
            value_width = value_width
                .max(fmt_commas(band.first).len())
                .max(fmt_commas(band.last).len())
                .max(fmt_commas(band.last - band.first).len())
                .max(fmt_commas_f64(mean, decimals).len());
            count_width = count_width.max(fmt_commas(band.count).len());
        }

        // Summary values share the value column.
        for stats in [table.overall(), table.trimmed()] {
            value_width = value_width
                .max(fmt_commas_f64(stats.mean, decimals).len())
                .max(fmt_commas_f64(stats.variance.sqrt(), decimals).len());
        }

        // Summary labels sit across the whole label region;
        // widen the second cell (or the only cell) to fit.
        let trim_label = summary_label("stdev", table.trim_range(), style).len();
        let region = zpn_width + if two_cell { 1 + frac_width } else { 0 };
        if trim_label > region {
            let grow = trim_label - region;
            if two_cell {
                frac_width += grow;
            } else {
                zpn_width += grow;
            }
        }

        Layout {
            zpn_width,
            frac_width: if two_cell { frac_width } else { 0 },
            value_width,
            count_width,
            decimals,
        }
    }
}

/// A summary-row label: the prefix alone (`mean`, `stdev`),
/// or `prefix lo..hi` over the trim extent (collapsed to one
/// name when the extent is a single band).
fn summary_label(prefix: &str, range: Option<(Boundary, Boundary)>, style: BandLabels) -> String {
    match range {
        None => prefix.to_string(),
        Some((lo, hi)) if lo == hi => format!("{prefix} {}", trim_name(lo, style)),
        Some((lo, hi)) => format!(
            "{prefix} {}..{}",
            trim_name(lo, style),
            trim_name(hi, style)
        ),
    }
}

/// Render `table` as text: header, one row per populated
/// band, a blank line, then overall and trimmed mean/stdev
/// rows. Lines end with `\n`.
///
/// - `layout` fixes the column widths — [`Layout::measure`]
///   for snug columns, [`Layout::DEMO_LEGACY`] for the demo's
///   historical shape.
/// - The trimmed rows are omitted when nothing below the n2
///   cut is populated.
pub fn render_band_table<const CAP: usize>(
    table: &BandTable<CAP>,
    style: BandLabels,
    layout: &Layout,
) -> String {
    let two_cell = layout.frac_width > 0;
    let zw = layout.zpn_width;
    let fw = layout.frac_width;
    let vw = layout.value_width;
    let cw = layout.count_width;
    let region = zw + if two_cell { 1 + fw } else { 0 };
    let mut out = String::new();

    // Header row.
    out.push_str(&format!(
        "{:<region$} {:>vw$} {:>vw$} {:>vw$} {:>cw$} {:>vw$}\n",
        "", "first", "last", "range", "count", "mean"
    ));

    // Band rows.
    for (index, band) in table.bands().iter().enumerate() {
        if band.count == 0 {
            continue;
        }
        let Some(upper) = table.ladder().get(index + 1) else {
            continue;
        };
        let labels = if two_cell {
            let frac = match upper {
                Boundary::Min | Boundary::Max => String::new(),
                _ => frac_label(upper),
            };
            format!("{:<zw$} {frac:<fw$}", zpn_label(upper))
        } else {
            format!("{:<zw$}", boundary_label(upper, style))
        };
        let mean = band.weighted_sum / band.count as f64;
        out.push_str(&format!(
            "{labels} {:>vw$} {:>vw$} {:>vw$} {:>cw$} {:>vw$}\n",
            fmt_commas(band.first),
            fmt_commas(band.last),
            fmt_commas(band.last - band.first),
            fmt_commas(band.count),
            fmt_commas_f64(mean, layout.decimals),
        ));
    }

    out.push('\n');

    // Summary rows: overall, then trimmed when populated.
    let overall = table.overall();
    let decimals = layout.decimals;
    out.push_str(&format!(
        "{:<region$} {:>vw$}\n",
        "mean",
        fmt_commas_f64(overall.mean, decimals)
    ));
    out.push_str(&format!(
        "{:<region$} {:>vw$}\n",
        "stdev",
        fmt_commas_f64(overall.variance.sqrt(), decimals)
    ));
    if let Some(range) = table.trim_range() {
        let trimmed = table.trimmed();
        out.push_str(&format!(
            "{:<region$} {:>vw$}\n",
            summary_label("mean", Some(range), style),
            fmt_commas_f64(trimmed.mean, decimals)
        ));
        out.push_str(&format!(
            "{:<region$} {:>vw$}\n",
            summary_label("stdev", Some(range), style),
            fmt_commas_f64(trimmed.variance.sqrt(), decimals)
        ));
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // OK: tests panic on setup failure by design
mod tests {
    use super::*;
    use crate::bands::{Ladder, RankSplit};
    use crate::{Config, Histogram};

    /// The z4/n10 ladder's rendered labels must match
    /// iiac-perf's documented lists, both styles.
    #[test]
    fn labels_match_iiac_perf() {
        let ladder = Ladder::new(4, 10).unwrap();
        let zpn_names: Vec<String> = ladder.iter().map(zpn_label).collect();
        assert_eq!(
            zpn_names,
            [
                "min", "z4", "z3", "z2", "p10", "p20", "p30", "p40", "p50", "p60", "p70", "p80",
                "p90", "n2", "n3", "n4", "n5", "n6", "n7", "n8", "n9", "n10", "max",
            ]
        );
        let frac_names: Vec<String> = ladder.iter().map(frac_label).collect();
        assert_eq!(
            frac_names,
            [
                "min",
                "0.000_1",
                "0.001",
                "0.01",
                "0.10",
                "0.20",
                "0.30",
                "0.40",
                "0.50",
                "0.60",
                "0.70",
                "0.80",
                "0.90",
                "0.99",
                "0.999",
                "0.999_9",
                "0.999_99",
                "0.999_999",
                "0.999_999_9",
                "0.999_999_99",
                "0.999_999_999",
                "0.999_999_999_9",
                "max",
            ]
        );
    }

    /// `Both` pads zpn to 3 and appends the fraction;
    /// min/max collapse to the bare name.
    #[test]
    fn both_label_shapes() {
        assert_eq!(
            boundary_label(Boundary::N(4), BandLabels::Both),
            "n4  0.999_9"
        );
        assert_eq!(
            boundary_label(Boundary::N(10), BandLabels::Both),
            "n10 0.999_999_999_9"
        );
        assert_eq!(boundary_label(Boundary::P(5), BandLabels::Both), "p50 0.50");
        assert_eq!(boundary_label(Boundary::Min, BandLabels::Both), "min");
        assert_eq!(boundary_label(Boundary::Max, BandLabels::Both), "max");
        assert_eq!(boundary_label(Boundary::Z(3), BandLabels::Zpn), "z3");
        assert_eq!(boundary_label(Boundary::Z(3), BandLabels::Frac), "0.001");
    }

    /// Trim names use the bare zpn form under `Both`.
    #[test]
    fn trim_name_bare() {
        assert_eq!(trim_name(Boundary::Z(4), BandLabels::Both), "z4");
        assert_eq!(trim_name(Boundary::N(2), BandLabels::Frac), "0.99");
    }

    /// A fully predictable table (every value identical)
    /// renders byte-identically to the demo's format strings.
    #[test]
    fn render_matches_demo_format() {
        const CFG: Config = match Config::new(2, 8) {
            Ok(config) => config,
            Err(_) => panic!("invalid test config"),
        };
        const BUCKETS: usize = CFG.total_buckets();
        const LADDER: Ladder = match Ladder::new(2, 2) {
            Ok(ladder) => ladder,
            Err(_) => panic!("invalid test ladder"),
        };
        const CAP: usize = LADDER.band_count();

        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        hist.record_n(5, 10);
        let table: crate::BandTable<CAP> =
            crate::BandTable::build(LADDER, RankSplit::new(), || hist.buckets()).unwrap();

        let rendered = render_band_table(&table, BandLabels::Both, &Layout::DEMO_LEGACY);

        // Expected, straight from the demo's format strings.
        let mut expected = String::new();
        expected.push_str(&format!(
            "{:<4} {:<13} {:>14} {:>14} {:>14} {:>12} {:>14}\n",
            "", "", "first", "last", "range", "count", "mean"
        ));
        for decile in 1..=9u8 {
            expected.push_str(&format!(
                "{:<4} {:<13} {:>14} {:>14} {:>14} {:>12} {:>14}\n",
                format!("p{decile}0"),
                format!("0.{decile}0"),
                "5",
                "5",
                "0",
                "1",
                "5.0"
            ));
        }
        expected.push_str(&format!(
            "{:<4} {:<13} {:>14} {:>14} {:>14} {:>12} {:>14}\n",
            "max", "", "5", "5", "0", "1", "5.0"
        ));
        expected.push('\n');
        expected.push_str(&format!("{:<18} {:>14}\n", "mean", "5.0"));
        expected.push_str(&format!("{:<18} {:>14}\n", "stdev", "0.0"));
        expected.push_str(&format!("{:<18} {:>14}\n", "mean p10..p90", "5.0"));
        expected.push_str(&format!("{:<18} {:>14}\n", "stdev p10..p90", "0.0"));
        assert_eq!(rendered, expected);
    }

    /// Measured layout is snug and renders aligned rows.
    #[test]
    fn measured_layout() {
        const CFG: Config = match Config::new(2, 8) {
            Ok(config) => config,
            Err(_) => panic!("invalid test config"),
        };
        const BUCKETS: usize = CFG.total_buckets();
        const LADDER: Ladder = match Ladder::new(2, 2) {
            Ok(ladder) => ladder,
            Err(_) => panic!("invalid test ladder"),
        };
        const CAP: usize = LADDER.band_count();

        let mut counts = [0u32; BUCKETS];
        let mut hist = Histogram::new(CFG, &mut counts).unwrap();
        hist.record_n(5, 10);
        let table: crate::BandTable<CAP> =
            crate::BandTable::build(LADDER, RankSplit::new(), || hist.buckets()).unwrap();

        let layout = Layout::measure(&table, BandLabels::Both, 1);
        // Snug: zpn 3 ("p10"), value 5 ("first"), count 5
        // ("count"); frac widened so "stdev p10..p90" fits the
        // label region (3 + 1 + frac >= 14).
        assert_eq!(layout.zpn_width, 3);
        assert_eq!(layout.value_width, 5);
        assert_eq!(layout.count_width, 5);
        assert!(layout.zpn_width + 1 + layout.frac_width >= "stdev p10..p90".len());

        let rendered = render_band_table(&table, BandLabels::Both, &layout);
        // Header and band rows all share one width.
        let lines: Vec<&str> = rendered.lines().collect();
        let header_len = lines[0].len();
        for line in &lines[1..11] {
            assert_eq!(line.len(), header_len, "line: {line:?}");
        }
    }
}
