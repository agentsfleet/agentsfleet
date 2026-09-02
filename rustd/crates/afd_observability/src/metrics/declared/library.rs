//! What an authenticated catalogue read cost, stage by stage.
//!
//! Six stages behind three surfaces, recorded as fixed-cardinality counters
//! rather than spans. `docs/architecture/observability.md` §Library read stages
//! carries the reasoning: a six-stage read emitting spans would spend most of a
//! second's span admission on its own timing and evict the server-error spans
//! the budget protects.
//!
//! Duration and observations are two counters and not a summary, deliberately.
//! `rate(duration)/rate(observations)` is the mean cost of a STAGE, and `sql`
//! fires twice per registry read — dividing by requests would halve its
//! apparent cost.

use crate::metrics::family::{CounterKind, Declared};

/// ÷ observations = mean stage cost.
///
/// Labels: `surface,stage`.
pub const LIBRARY_STAGE_DURATION_SECONDS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_library_stage_duration_seconds_total");

/// The denominator above.
///
/// Labels: `surface,stage`.
pub const LIBRARY_STAGE_OBSERVATIONS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_library_stage_observations_total");

/// Non-`ok` outcomes per surface.
///
/// Labels: `surface,outcome`.
pub const LIBRARY_READ_OUTCOME_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_library_read_outcome_total");

/// `timeout` = pool starved; `error` = datastore down.
///
/// Labels: `pool_result`.
pub const LIBRARY_POOL_RESULT_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_library_pool_result_total");

/// Hit ratio of the global catalogue cache.
///
/// Labels: `cache`.
pub const LIBRARY_CACHE_OUTCOME_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_library_cache_outcome_total");

/// Response bytes per surface.
///
/// Labels: `surface`.
pub const LIBRARY_PAYLOAD_BYTES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_library_payload_bytes_total");

/// Rows served per surface.
///
/// Labels: `surface`.
pub const LIBRARY_RESULTS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_library_results_total");
