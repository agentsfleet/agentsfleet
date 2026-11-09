//! Assemble the existing terminal statement and its returned billing facts.

use af_core::id::Uuid7;
use af_wire::report::ReportRequest;
use afd_billing::{Nanos, rates::Posture};
use jiff::Timestamp;

use super::{Reconciled, RunTokens};
use crate::lease::{
    commit::TerminalReport,
    pull::LeaseService,
    settle::Reported,
    tool_trace::{self, TraceOwner},
    usage::Pricing,
    verdict::{Terminal, Verdict},
};

/// The lease a report is about, by its three names and its row.
#[derive(Clone, Copy)]
pub(super) struct Subject<'a> {
    pub(super) id: &'a Uuid7,
    /// The checkpoint's validated event identifier.
    pub(super) last_event: &'a Uuid7,
    pub(super) key: &'a Uuid7,
    pub(super) runner: &'a Uuid7,
    pub(super) row: &'a Reported,
}

/// Everything the report's transaction needs, assembled from the wire request.
///
/// Separate from [`LeaseService::report`] so the verb reads as its four steps. The
/// two `try_from` saturations are the request's own numbers: a runner reports
/// its counts and nothing upstream bounds them, so a value past `i64` is
/// clamped rather than refused — the run happened either way, and refusing it
/// here would lose the answer over a telemetry field.
pub(super) fn terminal<'a>(
    lease: Subject<'a>,
    pricing: &'a Pricing,
    request: &'a ReportRequest<'a>,
    tool_calls: Option<&'a str>,
    now: Timestamp,
) -> TerminalReport<'a> {
    TerminalReport {
        lease_id: lease.id,
        runner_id: lease.runner,
        lease: lease.row,
        lease_key: lease.key,
        requests: &request.requests.0,
        pricing,
        outcome: Terminal {
            verdict: Verdict::of(
                request.outcome,
                request.failure_reason,
                request.failure_detail.as_ref(),
            ),
            response_text: request.response_text.as_ref(),
            tokens: i64::try_from(request.tokens).unwrap_or(i64::MAX),
            wall_ms: i64::try_from(request.telemetry.wall_ms).unwrap_or(i64::MAX),
            tool_calls,
        },
        last_event_id: lease.last_event,
        last_response: request.checkpoint.last_response.as_ref(),
        held_until: request.held_until,
        task_settlement: None,
        folded_through_sequence: request.folded_through_sequence,
        now,
    }
}

/// The run's tool trace as the event row stores it, fenced to `lease`.
pub(super) fn stored_trace(lease: &Reported, request: &ReportRequest<'_>) -> Option<String> {
    let owner = TraceOwner {
        fleet_id: &lease.fleet_id,
        event_id: &lease.event_id,
        fence: lease.fence.as_i64(),
    };
    tool_trace::stored(request.tool_calls, owner)
}

/// The lease's facts, as the caller of the verb receives them.
///
/// Takes the lease BY VALUE: every string field moves into the answer rather
/// than being cloned, which is why the two call sites read the lease's columns
/// through this and not before it.
pub(super) fn reconciled(
    lease: Reported,
    charged: Nanos,
    repeated: bool,
    tokens: RunTokens,
    completed_at: Option<Timestamp>,
) -> Reconciled {
    Reconciled {
        tokens,
        charged,
        fleet_id: lease.fleet_id,
        workspace_id: lease.workspace_id,
        tenant_id: lease.tenant_id,
        event_id: lease.event_id,
        posture: lease.posture,
        provider: lease.provider,
        model: lease.model,
        repeated,
        event_created_at: lease.event_created_at,
        lease_created_at: lease.lease_created_at,
        completed_at,
    }
}

/// The posture this lease was issued under.
///
/// An unrecognised stored spelling meters as `Platform`, which is what
/// `parsePosture` does and is deliberately NOT corrected here. The column is
/// written only by the issue path, from a `Posture`'s own `as_str`, so a value
/// that will not parse means the row was edited out of band — and what billing
/// should charge for such a row is a product decision, not a parse decision.
/// It is logged rather than absorbed silently.
pub(super) fn posture_of(lease: &Reported) -> Posture {
    Posture::parse(&lease.posture).unwrap_or_else(|| {
        let fleet = lease.fleet_id.as_str();
        let stored = lease.posture.as_str();
        tracing::warn!(
            fleet_id = fleet,
            posture = stored,
            event = "report_posture_unparseable",
            "the stored posture does not parse; metering falls back to platform"
        );
        Posture::Platform
    })
}

impl LeaseService {
    pub(super) async fn prepare_task_report(
        &self,
        runner_id: &Uuid7,
        lease_id: &Uuid7,
        request: &ReportRequest<'_>,
    ) -> crate::Result<Option<crate::task::PreparedReport>> {
        match &self.task_reports {
            Some(owner) => {
                owner
                    .prepare_report(
                        crate::task::TaskLease {
                            runner_id,
                            lease_id,
                            fencing_token: request.fencing_token,
                        },
                        request,
                    )
                    .await
            }
            None => Ok(None),
        }
    }
}
