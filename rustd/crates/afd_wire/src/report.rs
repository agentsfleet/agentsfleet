//! Lease renewal and the terminal execution report — the metering-bearing half.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::tool_trace::RawToolTrace;
#[cfg(feature = "openapi")]
use crate::tool_trace::ToolTrace;

/// The terminal verdict a runner reports.
///
/// Mirrors the event statuses a RUNNER can produce; the daemon-side statuses are
/// never runner-reported.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The run finished and produced a result.
    Processed,
    /// The run failed.
    FleetError,
}

impl Outcome {
    /// The verdict as it is spelled on the wire and in a stored row.
    ///
    /// The same bytes `serde` writes — the `rename_all` above and this must
    /// agree, because a product event groups runs by this string while the
    /// event row stores the serialized one, and a dashboard joining the two
    /// would silently find nothing if they drifted.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Processed => "processed",
            Self::FleetError => "fleet_error",
        }
    }
}

/// Why a run failed, at the granularity the classification site knows.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// The sandbox could not be established to the assigned posture.
    StartupPosture,
    /// Policy refused an action the run attempted.
    PolicyDeny,
    /// The run exceeded its wall-clock deadline and was killed.
    TimeoutKill,
    /// The run exceeded its memory ceiling and was killed.
    OomKill,
    /// The run exceeded another resource ceiling and was killed.
    ResourceKill,
    /// The runner process itself failed.
    RunnerCrash,
    /// The connection to the child was lost.
    TransportLoss,
    /// Filesystem isolation refused an access.
    LandlockDeny,
    /// The lease expired before the run finished.
    LeaseExpired,
    /// Renewal was refused and the child was terminated.
    RenewalTerminate,
    /// The run exceeded its spend budget.
    BudgetBreach,
}

/// `POST /v1/runners/me/leases/{lease_id}/renew` reply.
///
/// The authoritative new kill deadline. A non-`200` means stop renewing and kill
/// the child — the run is over.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenewResponse {
    /// Epoch milliseconds of the new deadline.
    pub lease_expires_at: i64,
}

/// `POST /v1/runners/me/leases/{lease_id}/renew` request.
///
/// Cumulative token counts for the run so far, never deltas. Only the
/// difference since the last renewal is charged, so a retry that re-sends the
/// same totals charges nothing new.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RenewRequest {
    /// Cumulative prompt tokens.
    pub input_tokens: u32,
    /// Cumulative cache-read tokens.
    pub cached_input_tokens: u32,
    /// Cumulative completion tokens.
    pub output_tokens: u32,
}

/// Latency the runner observed for one run.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportTelemetry {
    /// Milliseconds until the first token arrived.
    pub time_to_first_token_ms: u32,
    /// Total wall-clock milliseconds.
    pub wall_ms: u64,
}

/// Session resume cursor written to the fleet's stored session context.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportCheckpoint<'a> {
    /// The last event this session processed.
    #[serde(borrow)]
    pub last_event_id: Cow<'a, str>,
    /// The last response it produced.
    #[serde(borrow)]
    pub last_response: Cow<'a, str>,
}

/// `POST /v1/runners/me/reports` — one batched write keyed by event id.
//
// The fencing token is echoed and verified: a reclaimed holder carrying a token
// below the fleet's live sequence is refused. No runner id — the token owns the
// identity.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportRequest<'a> {
    /// The lease being reported on.
    #[serde(borrow)]
    pub lease_id: Cow<'a, str>,
    /// The event this report is about.
    #[serde(borrow)]
    pub event_id: Cow<'a, str>,
    /// Monotonic guard, verified against the fleet's live sequence.
    pub fencing_token: u64,
    /// The binary verdict.
    pub outcome: Outcome,
    /// The granular cause when the run failed.
    pub failure_reason: Option<FailureClass>,
    /// Human-readable cause, stored only on failure.
    #[serde(borrow)]
    pub failure_detail: Cow<'a, str>,
    /// The run's output.
    #[serde(borrow)]
    pub response_text: Cow<'a, str>,
    /// The run's total tokens, for reporting. Billing charges the three
    /// cumulative fields below, which settle against the usage ledger.
    pub tokens: u64,
    /// Cumulative prompt tokens for the whole run.
    pub input_tokens: u32,
    /// Cumulative cache-read tokens for the whole run.
    pub cached_input_tokens: u32,
    /// Cumulative completion tokens for the whole run.
    pub output_tokens: u32,
    /// Latency the runner observed.
    pub telemetry: ReportTelemetry,
    /// Where to resume this session.
    #[serde(borrow)]
    pub checkpoint: ReportCheckpoint<'a>,
    /// Every tool call the run made, kept with its answer. Absent from
    /// runners that do not record one. A trace that is not one, or that
    /// breaks a bound, is dropped and the report still settles.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<ToolTrace>))]
    pub tool_calls: Option<RawToolTrace<'a>>,
}

/// `POST /v1/runners/me/reports` reply.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportResponse {
    /// Whether the write landed.
    pub ok: bool,
}

#[cfg(test)]
#[path = "report/tests.rs"]
mod tests;
