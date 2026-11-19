//! Assemble the existing terminal statement and its returned billing facts.

use af_core::id::Uuid7;
use af_wire::report::ReportRequest;
use jiff::Timestamp;

use super::{Reconciled, RunTokens};
use crate::run::{
    commit::TerminalReport,
    pull::RunService,
    settle::Reported,
    tool_trace::{self, TraceOwner},
    usage::Pricing,
    verdict::{Terminal, Verdict},
};

/// The run a report is about, by its three names and its row.
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
/// Separate from [`RunService::report_with`] so the verb reads as its four steps. The
/// two `try_from` saturations are the request's own numbers: a runner reports
/// its counts and nothing upstream bounds them, so a value past `i64` is
/// clamped rather than refused — the run happened either way, and refusing it
/// here would lose the answer over a telemetry field.
pub(super) fn terminal<'a>(
    run: Subject<'a>,
    pricing: &'a Pricing,
    request: &'a ReportRequest<'a>,
    tool_calls: Option<&'a str>,
    now: Timestamp,
) -> TerminalReport<'a> {
    TerminalReport {
        run_id: run.id,
        runner_id: run.runner,
        run: run.row,
        run_key: run.key,
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
        last_event_id: run.last_event,
        last_response: request.checkpoint.last_response.as_ref(),
        held_until: request.held_until,
        task_settlement: None,
        folded_through_sequence: request.folded_through_sequence,
        now,
    }
}

/// The run's tool trace as the event row stores it, fenced to `run`.
pub(super) fn stored_trace(run: &Reported, request: &ReportRequest<'_>) -> Option<String> {
    let owner = TraceOwner {
        fleet_id: &run.fleet_id,
        event_id: &run.event_id,
        fence: run.fence.as_i64(),
    };
    tool_trace::stored(request.tool_calls, owner)
}

/// The run's facts, as the caller of the verb receives them.
///
/// Takes the run BY VALUE: every string field moves into the answer rather
/// than being cloned, which is why the two call sites read the run's columns
/// through this and not before it.
pub(super) fn reconciled(
    run: Reported,
    repeated: bool,
    tokens: RunTokens,
    completed_at: Option<Timestamp>,
) -> Reconciled {
    Reconciled {
        hold: None,
        tokens,
        fleet_id: run.fleet_id,
        workspace_id: run.workspace_id,
        tenant_id: run.tenant_id,
        event_id: run.event_id,
        posture: run.posture,
        provider: run.provider,
        model: run.model,
        repeated,
        event_created_at: run.event_created_at,
        run_created_at: run.run_created_at,
        completed_at,
    }
}

impl RunService {
    pub(super) async fn prepare_task_report(
        &self,
        runner_id: &Uuid7,
        run_id: &Uuid7,
        request: &ReportRequest<'_>,
    ) -> crate::Result<Option<crate::task::PreparedReport>> {
        match &self.task_reports {
            Some(owner) => {
                owner
                    .prepare_report(
                        crate::task::TaskRun {
                            runner_id,
                            run_id,
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
