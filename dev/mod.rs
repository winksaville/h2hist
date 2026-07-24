//! Code shared by the test, bench, and demo consumers.
//!
//! - Not part of the crate: each consumer pulls this in with
//!   `#[path = "../dev/mod.rs"] mod dev;`, so nothing here
//!   reaches the published API and no manifest feature is
//!   needed to keep it out.
//! - Holds what more than one consumer would otherwise copy:
//!   the deterministic PRNG, the synthetic latency stream, and
//!   the constants both are shaped by.
//!
//! Every consumer includes the whole module but uses only part
//! of it, so unused items are expected rather than a defect.

#![allow(dead_code)]

pub mod consts;
pub mod rng;
pub mod stream;
