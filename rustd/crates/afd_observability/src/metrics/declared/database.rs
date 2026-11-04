//! The role pools' current capacity and the waits they actually served.

use crate::metrics::family::{Declared, GaugeKind, HistogramKind};

/// Connections and pending acquisitions, by role and state.
pub const POOL_CONNECTIONS: Declared<GaugeKind> = Declared::new("agentsfleet_db_pool_connections");

/// End-to-end acquisition time, including the existing stall retry.
pub const POOL_ACQUIRE_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_db_pool_acquire_duration_seconds");
