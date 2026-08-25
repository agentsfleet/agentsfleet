//! The long-lived tasks this process supervises.
//!
//! Runtime truth and nothing else. What [`crate::Supervisor::inventory`] must
//! equal once boot has finished, so a task added to boot without a name here —
//! or a name here that boot never spawns — is a failing test rather than a
//! comment nobody re-read.
//!
//! # What is deliberately NOT here
//!
//! The porting ledger — which Zig thread became what, and which milestone owes
//! the rows this build does not run yet — used to live in this file. It is
//! project metadata, and a daemon has no use for it: renumbering a milestone
//! would have meant editing a shipped binary, and a row that landed would have
//! left a stale string compiled into every release.
//!
//! It now lives where it belongs and is still machine-checked: the table is in
//! `docs/architecture/concurrency.md`, and `tests/daemon.rs` holds the same
//! rows as test data and asserts every one has a disposition. Tests are not
//! shipped, so the check survives and the binary carries nothing.

/// The supervised name for the Redis pub/sub pump.
pub const HUB_PUMP: &str = "hub_pump";

/// The supervised name for the span exporter's flush loop.
pub const OTLP_EXPORT: &str = "otlp_export";

/// Every long-lived task a fully booted daemon supervises, in spawn order.
///
/// The accept loop is not here: it is spawned by [`crate::serve::boot`] and is
/// the server rather than a background task, so it is asserted where it is
/// created instead of being listed as something boot must go and find.
pub const BACKGROUND_TASKS: &[&str] = &[HUB_PUMP, OTLP_EXPORT];
