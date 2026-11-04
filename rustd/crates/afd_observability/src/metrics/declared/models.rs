//! Model-call latency and bounded snapshots of the product catalogue.
use crate::metrics::family::{CounterKind, Declared, GaugeKind, HistogramKind};
/// Complete stream latency; independent of invocation and tool time.
pub const CALL_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_model_call_duration_seconds");
/// Time to first nonempty text; absent for a call that produced none.
pub const FIRST_TOKEN_DURATION: Declared<HistogramKind> =
    Declared::new("agentsfleet_model_first_token_duration_seconds");
/// Verified context and configured output limits.
pub const LIMITS: Declared<GaugeKind> = Declared::new("agentsfleet_model_limit_tokens");
/// Features the configured route supports, not inferred model-quality claims.
pub const ROUTES: Declared<GaugeKind> = Declared::new("agentsfleet_model_route_capability");
/// Age of the last successful catalogue snapshot.
pub const SNAPSHOT_AGE: Declared<GaugeKind> =
    Declared::new("agentsfleet_model_catalog_snapshot_age_seconds");
/// Whether catalogue rows exceeded the declared snapshot budget.
pub const CLIPPED: Declared<GaugeKind> = Declared::new("agentsfleet_model_catalog_clipped");

/// Observed input and output tokens, including failed calls with known usage.
pub const TOKENS: Declared<CounterKind> = Declared::new("agentsfleet_model_tokens_total");
/// Cache reads are a subset of input, not another additive direction.
pub const CACHE_READ: Declared<CounterKind> =
    Declared::new("agentsfleet_model_cache_read_tokens_total");
/// Cache writes are a subset of input.
pub const CACHE_WRITE: Declared<CounterKind> =
    Declared::new("agentsfleet_model_cache_write_tokens_total");
