//! Lease renewal and the terminal execution report — the metering-bearing half.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

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

/// A clean finish. Empty by construction — the run's numbers live on the result
/// itself, shared by both verdicts.
///
/// The braces are load-bearing and cannot become a unit struct: serde encodes a
/// unit struct as `null`, so `Outcome::Completed` would serialize as
/// `{"completed":null}` where the wire carries `{"completed":{}}`.
/// `test_wire_roundtrip_all_fixtures` fails on that change, which is what makes
/// this reason checkable rather than asserted.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "the empty braces are the wire encoding; a unit struct serializes as null"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Completed {}

/// Why a run failed.
///
/// `class` is null only when the peer reported a failure without classifying it.
/// A cause is never guessed from a bare failure.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Failure<'a> {
    /// The classified cause, when there is one.
    pub class: Option<FailureClass>,
    /// Human-readable cause from the classification site.
    #[serde(borrow)]
    pub detail: Cow<'a, str>,
}

/// The run's verdict. `Completed` carries no cause because a clean run has none.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultOutcome<'a> {
    /// The run finished cleanly.
    Completed(Completed),
    /// The run failed.
    #[serde(borrow)]
    Failed(Failure<'a>),
}

/// The terminal stage result the runner produces and the report consumes.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionResult<'a> {
    /// Whether the run finished or failed, and why.
    #[serde(borrow)]
    pub outcome: ResultOutcome<'a>,
    /// The run's output.
    #[serde(borrow)]
    pub content: Cow<'a, str>,
    /// Total tokens, for reporting rather than billing.
    pub token_count: u64,
    /// Wall-clock seconds the run took.
    pub wall_seconds: u64,
    /// Peak resident bytes observed.
    pub memory_peak_bytes: u64,
    /// Milliseconds the run spent throttled.
    pub cpu_throttled_ms: u64,
    /// Cumulative prompt tokens for the whole run.
    pub input_tokens: u64,
    /// Cumulative cache-read tokens for the whole run.
    pub cached_input_tokens: u64,
    /// Cumulative completion tokens for the whole run.
    pub output_tokens: u64,
}

/// `POST /v1/runners/me/leases/{lease_id}/renew` reply.
///
/// The authoritative new kill deadline. A non-`200` means stop renewing and kill
/// the child — the run is over.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenewResponse {
    /// Epoch milliseconds of the new deadline.
    pub lease_expires_at: i64,
}

/// `POST /v1/runners/me/leases/{lease_id}/renew` request.
///
/// CUMULATIVE token counts for the run so far, not deltas. The control plane
/// charges the difference since the lease's last-metered cursor, so a fail-safe
/// retry re-sending the same cumulatives charges approximately nothing.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RenewRequest {
    /// Cumulative prompt tokens.
    pub input_tokens: u32,
    /// Cumulative cache-read tokens.
    pub cached_input_tokens: u32,
    /// Cumulative completion tokens.
    pub output_tokens: u32,
}

/// Latency telemetry the runner observed for one execution.
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
///
/// The fencing token is echoed and verified: a reclaimed holder carrying a token
/// below the fleet's live sequence is refused. No runner id — the token owns the
/// identity.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportRequest<'a> {
    /// The lease being reported on.
    #[serde(borrow)]
    pub lease_id: Cow<'a, str>,
    /// The event that was executed.
    #[serde(borrow)]
    pub event_id: Cow<'a, str>,
    /// Monotonic guard, verified against the fleet's live sequence.
    pub fencing_token: u64,
    /// The binary verdict.
    pub outcome: Outcome,
    /// The granular cause when the run failed.
    pub failure_reason: Option<FailureClass>,
    /// Human-readable cause, persisted only on failure.
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
}

/// `POST /v1/runners/me/reports` reply.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportResponse {
    /// Whether the write landed.
    pub ok: bool,
}

#[cfg(test)]
#[path = "report/tests.rs"]
mod tests;
