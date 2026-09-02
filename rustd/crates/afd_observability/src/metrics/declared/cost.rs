//! What a run cost, in the vocabulary the standard already has for it.
//!
//! The five dotted families, and the only ones that are not `agentsfleet_`
//! prefixed. Two of the names are the standard's own and three are ours, and
//! `docs/architecture/observability.md` §Metrics stay semantic carries why each
//! divergence exists: a `ReportRequest` counts tokens cumulatively over a whole
//! agent run rather than one provider call, so run totals cannot honestly wear
//! the GenAI client-call names.
//!
//! They are also the only families drawn from the shared cost sub-budget rather
//! than owning a series ceiling of their own, which is what the census's
//! `shared:cost` policy says.

use crate::metrics::family::{Declared, CounterKind, HistogramKind};

/// Runner wall time per invocation.
///
/// Labels: `gen_ai.request.model`.
pub const INVOKE_AGENT_DURATION: Declared<HistogramKind> =
    Declared::new("gen_ai.invoke_agent.duration");

/// Token spend per invocation, by `gen_ai.token.type`.
///
/// Labels: `gen_ai.request.model`.
pub const INVOKE_AGENT_TOKEN_USAGE: Declared<HistogramKind> =
    Declared::new("agentsfleet.invoke_agent.token.usage");

/// Cache-read subset of input tokens.
///
/// Labels: `gen_ai.request.model`.
pub const INVOKE_AGENT_CACHE_READ_TOKEN_USAGE: Declared<HistogramKind> =
    Declared::new("agentsfleet.invoke_agent.cache_read.token.usage");

/// Nanocredit spend by charge class.
///
/// Labels: `gen_ai.request.model`.
pub const BILLING_CREDIT_CONSUMED: Declared<CounterKind> =
    Declared::new("agentsfleet.billing.credit.consumed");

/// Exporter self-observability: ring + aggregation loss.
pub const TELEMETRY_SAMPLES_DROPPED: Declared<CounterKind> =
    Declared::new("agentsfleet.telemetry.samples_dropped");
