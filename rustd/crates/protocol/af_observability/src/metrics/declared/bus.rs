//! Signal-bus subscriptions told that frames may be missing.
use crate::metrics::family::{CounterKind, Declared};

/// Subscriptions that received a gap, by what caused it.
pub const GAPS: Declared<CounterKind> = Declared::new("agentsfleet_bus_gaps_total");
