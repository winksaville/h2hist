//! `no_std`, no-alloc, HdrHistogram-style log-linear histogram.
//!
//! - h2 parameterization `(grouping_power, max_value_power)`:
//!   relative error ≤ 2⁻ᵍ, max value 2ⁿ−1, O(1) integer-only
//!   record path (clz + shift + saturating increment).
//! - The core borrows caller-supplied counts storage; analysis
//!   (quantiles, merge, iteration) stays off the hot path.
//! - `std` feature (default) is convenience-only; the core is
//!   `no_std`. See `ARCHITECTURE.md` for design and size
//!   tradeoffs.

#![cfg_attr(not(feature = "std"), no_std)]

mod analysis;
mod array;
mod bands;
mod config;
mod counter;
mod histogram;
mod stats;
mod table;

pub use analysis::{Bucket, Buckets};
pub use array::HistogramArray;
pub use bands::{Band, BandAssign, Boundary, Ladder, LadderIter, MidRank, RankSplit};
pub use config::{Config, Error};
pub use counter::Counter;
pub use histogram::Histogram;
pub use stats::Stats;
pub use table::BandTable;
