//! Record-path cost bench (hand-rolled, `harness = false`).
//!
//! - Times ns/record over a pre-generated heavy-tailed value
//!   stream; best of `REPS` passes per variant. The variant
//!   list, how to interpret each row, and the caveats are
//!   documented in README.md's Bench section.
//! - Purpose: absolute record cost vs a raw store, the
//!   iopsystems `histogram` crate (same h2 scheme), and
//!   `hdrhistogram`; plus diagnostic rows isolating the
//!   marginal cost of a hot-path total and the min/max/sum
//!   extras — the data behind those design decisions.

use std::hint::black_box;
use std::time::Instant;

use h2hist::{Error, Histogram};

#[path = "../dev/mod.rs"]
mod dev;

use dev::consts::{BUCKETS, CFG, GROUPING_POWER, HDR_SIGFIG, MAX_VALUE_POWER, SEED};
use dev::stream::HeavyTailed;

/// Samples per pass.
const LEN: usize = 8_000_000;
/// Timed passes per variant; best is reported.
const REPS: usize = 3;

/// Best-of-REPS wall time of `run`, reported as ns per value.
///
/// `stored` names what the variant writes per record (e.g.
/// `u32 counter`, `u64 sample`) — the width that matters on
/// 32-bit targets even when invisible on the x86_64 host.
fn bench(label: &str, stored: &str, mut run: impl FnMut()) {
    let mut best = f64::MAX;
    for _ in 0..REPS {
        let start = Instant::now();
        run();
        let ns = start.elapsed().as_nanos() as f64;
        let per = ns / LEN as f64;
        if per < best {
            best = per;
        }
    }
    println!("{label:<22} {stored:<12} {best:>9.3}");
}

/// Run all variants and print the comparison table.
fn main() -> Result<(), Error> {
    let values: Vec<u64> = HeavyTailed::new(SEED, CFG.max_value()).take(LEN).collect();

    println!(
        "record bench: g={GROUPING_POWER} n={MAX_VALUE_POWER} buckets={BUCKETS} len={LEN} reps={REPS} (best)"
    );
    println!("{:<22} {:<12} {:>9}", "variant", "stored", "ns/record");

    let mut store = vec![0u64; LEN];
    bench("raw streaming store", "u64 sample", || {
        for (slot, &value) in store.iter_mut().zip(values.iter()) {
            *slot = value;
        }
        black_box(&store);
    });

    let mut counts = [0u32; BUCKETS];
    let mut hist = Histogram::new(CFG, &mut counts)?;
    bench("h2hist u32", "u32 counter", || {
        for &value in &values {
            hist.record(value);
        }
        black_box(hist.total());
    });

    let mut counts_with_total = [0u32; BUCKETS];
    let mut hist_with_total = Histogram::new(CFG, &mut counts_with_total)?;
    bench("h2hist u32 + total", "u32 counter", || {
        let mut total = 0u64;
        for &value in &values {
            hist_with_total.record(value);
            total = total.saturating_add(1);
        }
        black_box(total);
    });

    let mut counts_u64 = [0u64; BUCKETS];
    let mut hist_u64 = Histogram::<u64>::new(CFG, &mut counts_u64)?;
    bench("h2hist u64", "u64 counter", || {
        for &value in &values {
            hist_u64.record(value);
        }
        black_box(hist_u64.total());
    });

    let mut raw_counts = [0u32; BUCKETS];
    bench("index_for + wrap u32", "u32 counter", || {
        for &value in &values {
            if let Some(cnt) = raw_counts.get_mut(CFG.index_for(value)) {
                *cnt = cnt.wrapping_add(1);
            }
        }
        black_box(&raw_counts);
    });

    let mut counts_extras = [0u32; BUCKETS];
    let mut hist_extras = Histogram::new(CFG, &mut counts_extras)?;
    bench("h2hist u32 + extras", "u32 counter", || {
        let (mut min_v, mut max_v, mut sum) = (u64::MAX, 0u64, 0u64);
        for &value in &values {
            hist_extras.record(value);
            min_v = min_v.min(value);
            max_v = max_v.max(value);
            sum = sum.wrapping_add(value);
        }
        black_box((min_v, max_v, sum, hist_extras.total()));
    });

    match histogram::Histogram::new(GROUPING_POWER, MAX_VALUE_POWER) {
        Ok(mut iop) => {
            bench("histogram u64", "u64 counter", || {
                for &value in &values {
                    // Values are pre-clamped, increment cannot fail.
                    let _ = iop.increment(value);
                }
                black_box(&iop);
            });
        }
        Err(error) => println!("histogram crate setup failed: {error:?}"),
    }

    match hdrhistogram::Histogram::<u64>::new_with_bounds(1, CFG.max_value(), HDR_SIGFIG) {
        Ok(mut hdr) => {
            bench(
                &format!("hdrhistogram {HDR_SIGFIG}sf"),
                "u64 counter",
                || {
                    for &value in &values {
                        // Values are pre-clamped, record cannot fail.
                        let _ = hdr.record(value);
                    }
                    black_box(hdr.len());
                },
            );
        }
        Err(error) => println!("hdrhistogram setup failed: {error:?}"),
    }

    Ok(())
}
