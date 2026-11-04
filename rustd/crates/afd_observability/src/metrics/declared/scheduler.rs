//! Schedule inventory and completed decisions.
use crate::metrics::family::{CounterKind, Declared, GaugeKind, HistogramKind};
/// Durably admitted fires, with replay distinct from new work.
pub const FIRES: Declared<CounterKind> = Declared::new("agentsfleet_schedule_fires_total");
/// Verified callbacks, including skipped and interrupted requests.
pub const CALLBACKS: Declared<CounterKind> = Declared::new("agentsfleet_schedule_callbacks_total");
/// Verified callback handling time.
pub const CALLBACK_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_schedule_callback_duration_seconds");
/// Delay after a known one-shot instant; recurring instants are not invented.
pub const DISPATCH_LAG: Declared<HistogramKind> =
    Declared::new("agentsfleet_schedule_dispatch_lag_seconds");
/// Reconciliation outcomes, including expired one-shots.
pub const SYNCS: Declared<CounterKind> = Declared::new("agentsfleet_schedule_syncs_total");
/// Current intent, sync and overdue-one-shot counts.
pub const SCHEDULES: Declared<GaugeKind> = Declared::new("agentsfleet_schedules");
