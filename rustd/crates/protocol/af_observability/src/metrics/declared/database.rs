//! The role pools' current capacity and the waits they actually served.

use crate::metrics::family::{CounterKind, Declared, GaugeKind, HistogramKind};

/// Connections and pending acquisitions, by role and state.
pub const POOL_CONNECTIONS: Declared<GaugeKind> = Declared::new("agentsfleet_db_pool_connections");

/// End-to-end acquisition time, including the existing stall retry.
pub const POOL_ACQUIRE_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_db_pool_acquire_duration_seconds");

/// Idle connections a checkout tested before handing out, by role. A
/// connection used within the last second goes out untested, so this moves
/// only after a pool sat quiet.
///
/// Labels: `role`.
pub const POOL_CHECKOUT_PINGS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_db_pool_checkout_pings_total");
