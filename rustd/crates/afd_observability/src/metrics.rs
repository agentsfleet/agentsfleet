//! The metric pipeline: what this daemon counts, and the shape it counts in.
//!
//! The span half of this crate was here first. This half exists because the
//! crate carried no instrument, no aggregation and no family registry — so a
//! transport plugged in at boot would have carried an empty payload.
//!
//! # Why a file is the contract
//!
//! The family set is parity data: the Rust daemon has to emit what the Zig one
//! emitted, byte for byte at the OTLP wire, or every dashboard built on those
//! names breaks on the swap. A contract that lives in Rust source can only be
//! graded by reading Rust source, so it lives in `docs/metrics.census.tsv` and
//! the registry is built FROM it. The parity test then grades the registry
//! against the same file in both directions, and a family on one side only is
//! named rather than quietly dropped.

pub mod registry;
