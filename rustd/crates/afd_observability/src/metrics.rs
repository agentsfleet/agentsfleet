//! The metric pipeline: what this daemon counts, and the shape it counts in.
//!
//! The span half of this crate was here first. This half exists because the
//! crate carried no instrument, no aggregation and no family registry — so a
//! transport plugged in at boot would have carried an empty payload.
//!
//! # Why a file is the contract
//!
//! The family set is wire data: every dashboard is built on these names, byte
//! for byte at the OTLP wire, so a rename breaks a panel silently. A list that
//! lives in Rust source can only be graded by reading Rust source, so it lives
//! in `docs/metrics.census.tsv` and the registry is built FROM it. The parity
//! test then grades the registry against the same file in both directions, and
//! a family on one side only is named rather than quietly dropped.

pub mod declared;
pub mod export;
pub mod family;
pub mod instrument;
pub mod label;
pub mod observed;
pub mod produced;
pub mod registry;
