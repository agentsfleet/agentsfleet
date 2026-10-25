//! Lease renewal and the terminal execution report — the metering-bearing half.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::tool_trace::RawToolTrace;
#[cfg(feature = "openapi")]
use crate::tool_trace::ToolTrace;

/// Final run outcome reported by the runner. Server-only statuses are excluded.
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
    /// The new deadline.
    #[serde(with = "crate::instant")]
    pub lease_expires_at: jiff::Timestamp,
    /// Highest usage record saved. Discard records through this number and resend later ones.
    /// An unreadable body saves no records, even when renewal succeeds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorded_through: Option<u32>,
}

/// Usage since the last confirmed renewal, one record per provider request. Repeating records
/// does not add charges. The server measures runtime separately.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct RenewRequest {
    /// One line per provider request, possibly none.
    #[garde(dive)]
    pub requests: crate::usage::RequestLines,
}

/// Latency the runner observed for one run.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportTelemetry {
    /// Milliseconds from the start of the run to its first answer chunk, or
    /// `0` when it sent none.
    ///
    /// The run starts once memory and the sandbox are ready, so setup
    /// is not counted here; `wall_ms` counts it.
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
    /// The run's total tokens, for the event history. Billing charges
    /// `requests`, never this sum.
    pub tokens: u64,
    /// The provider requests no renewal acknowledged yet, one line each.
    pub requests: crate::usage::RequestLines,
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
    /// Until when this runner holds the run's sandbox, frozen, for the
    /// fleet's next event. Absent when it holds none.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::instant::option"
    )]
    pub held_until: Option<jiff::Timestamp>,
}

/// `POST /v1/runners/me/reports` reply.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportResponse {
    /// Whether the write landed.
    pub ok: bool,
}

#[cfg(test)]
mod tests;
