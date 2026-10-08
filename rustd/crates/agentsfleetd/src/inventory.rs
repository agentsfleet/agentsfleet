//! The long-lived tasks this process supervises.
//!
//! Runtime truth and nothing else. What [`crate::Supervisor::inventory`] must
//! equal once boot has finished, so a task added to boot without a name here —
//! or a name here that boot never spawns — is a failing test rather than a
//! comment nobody re-read.
//!
//! # What is deliberately NOT here
//!
//! The task ledger — which milestone owes the rows this build does not run yet
//! — is project metadata, and a daemon has no use for it: renumbering a
//! milestone would mean editing a shipped binary, and a row that landed would
//! leave a stale string compiled into every release.
//!
//! It lives in `docs/architecture/concurrency.md`. What the binary owes is
//! checked in `tests/daemon.rs`, which asserts by name that boot supervises
//! exactly the tasks named here.

/// The supervised name for the Dragonfly pub/sub pump.
pub const HUB_PUMP: &str = "hub_pump";

/// The supervised name for the span exporter's flush loop.
pub const OTLP_EXPORT: &str = "otlp_export";

/// The task that delivers queued product events before the process exits.
///
/// Not the reporting itself — that is fire-and-forget on the client's own
/// background transport. This is its STOP: without it, events captured by the
/// last requests served are dropped when the client goes away.
pub const ANALYTICS_FLUSH: &str = "analytics_flush";

/// The task that delivers a fleet's answer back through the connector the
/// question arrived on.
///
/// Named for the stream it reads rather than for a verb, because that is what
/// an operator correlating a supervised task with a Dragonfly key needs it to
/// match — see [`crate::outbound`].
pub const OUTBOUND_WORKER: &str = crate::outbound::OUTBOUND_WORKER;

/// Every long-lived task a fully booted daemon supervises, in spawn order.
///
/// The accept loop is not here: it is spawned by [`crate::serve::boot`] and is
/// the server rather than a background task, so it is asserted where it is
/// created instead of being listed as something boot must go and find.
pub const BACKGROUND_TASKS: &[&str] = &[HUB_PUMP, OUTBOUND_WORKER, OTLP_EXPORT, ANALYTICS_FLUSH];
