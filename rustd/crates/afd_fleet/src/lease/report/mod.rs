//! The report verb: one runner's terminal result, from fence to acknowledgement.
//!
//! Two halves with a commit between them. Everything before
//! [`Leases::commit_report`](crate::lease::commit) can still refuse the report;
//! nothing after it can, because by then the lease is flipped, the tenant is
//! charged, and the run's answer is durable beside both.
//!
//! # Why the money is settled before the event row is closed
//!
//! The reverse order looks tidier — end the work, then bill for it — and it
//! loses money on the cap path. A run that reaches `MAX_RUNTIME` is racing
//! the reclaim sweep, and the sweep bumps `fencing_token`. Settle first and the
//! affinity lock inside the statement holds the sweep off until the charge
//! commits. Settle second and the sweep wins the fence, the settle is refused,
//! and the last slice of a twelve-hour run is never charged to anyone.
//!
//! So the fence — which is what authorizes reporting at all — is spent on the
//! money first, and the narrative log is closed afterwards from a position
//! where nothing can take it away. Since §7 that position is the same
//! TRANSACTION rather than merely the next statement: the result, the session
//! cursor and the freed slot commit with the charge or not at all, so there is
//! no interval in which a tenant is charged for a run whose answer was never
//! written. [`crate::lease::commit`] carries that argument.
//!
//! # What this verb does NOT do
//!
//! No activity publish and no delivery-queue append inside the settle. An owed
//! answer's obligation row does commit there, through
//! [`crate::lease::obligation`]; the publish and the append run after the
//! commit, best effort, and a failure is logged, never returned.
//! No OTLP spans either — the drained amount comes back as a VALUE, the way
//! [`afd_billing::Accounts::debit_receive`] answers one, and the caller
//! attaches the instrument. An exporter fused into the money path would be one
//! a report could not settle without.

use afd_core::id::Uuid7;
use afd_wire::report::ReportRequest;
use jiff::Timestamp;

use crate::error::{Result, lease_not_found, stale_fence};
use crate::lease::commit::{Committed, TerminalReport};
use crate::lease::pull::Plane;
use crate::lease::settle::Reported;
use crate::lease::tool_trace::{self, TraceOwner};
use crate::lease::usage::{self, Pricing};
use crate::lease::verdict::{Terminal, Verdict};
use afd_billing::biller::Release;
use afd_billing::rates::Posture;
use afd_billing::{Cumulative, Nanos};

/// What one settled report leaves its caller with.
///
/// A struct rather than a bare [`Nanos`] because the charge was never the only
/// thing the call learned. The report names a LEASE; everything else about the
/// run — which fleet and workspace and tenant, which event, which model under
/// which posture — was resolved by the load inside, and the caller would
/// otherwise run a second statement for facts this call already held.
///
/// # Why the identity travels up rather than the telemetry down
///
/// Both callers of this verb describe the finished run: one reports it to
/// product analytics, the other records its delivery span. Neither concern
/// belongs on the money path — an exporter fused into it would be one a report
/// could not settle without — so the money path answers with the facts and the
/// handler decides what to say about them.
///
/// Exhaustive on purpose, where most of this crate's public types are not: a
/// suite stubbing the lease plane has to ANSWER with one of these, and a
/// `non_exhaustive` struct cannot be built outside the crate that declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconciled {
    /// What the final slice drained. Zero on a repeated report, which charges
    /// nothing because the first one charged it.
    pub charged: Nanos,
    /// The fleet that ran.
    pub fleet_id: Uuid7,
    /// The workspace it belongs to.
    pub workspace_id: Uuid7,
    /// The tenant whose wallet the settle drew on.
    pub tenant_id: Uuid7,
    /// The event that was executed, as the LEASE row records it.
    ///
    /// The lease's copy, never the request's. A runner names an event in its
    /// report and this is the one the lease was issued for; where they could
    /// differ, the row is the authority.
    pub event_id: String,
    /// The billing posture the lease was issued under, in the stored spelling.
    pub posture: String,
    /// The provider resolved at issue, as configured.
    pub provider: String,
    /// The model resolved at issue.
    pub model: String,
    /// Whether this call produced the outcome or found it already durable.
    ///
    /// True when the runner re-sent a report whose response it never received.
    /// The run itself happened once, so everything a caller says ABOUT the run
    /// — the completion analytics, the cost meters, the delivery span — must
    /// fire on the first report and not on the repeat, or one run is counted
    /// as many. The report still answers success: a runner told to retry
    /// forever is how a finished answer gets thrown away.
    pub repeated: bool,
}

impl Plane {
    /// Record one runner's terminal result for a lease it holds.
    ///
    /// Answers what the final slice's usage line cost, so the caller can meter
    /// it, and
    /// the two identifiers that were only knowable INSIDE this call: the report
    /// names a lease, and which fleet and workspace that lease belonged to is
    /// something the load resolved. The caller needs both to report the run,
    /// and reading them again would be a second statement for a fact this one
    /// already had.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's with
    /// [`lease_not_found`](crate::error), and a holder the fleet has superseded
    /// with [`stale_fence`](crate::error) — neither writes anything. Also
    /// reports a datastore that would not answer, in which case the lease is
    /// left `active` for the runner to re-report against and nothing was
    /// logged.
    pub async fn report(
        &self,
        runner_id: &Uuid7,
        request: &ReportRequest<'_>,
        now: Timestamp,
    ) -> Result<Reconciled> {
        let lease_id = request.lease_id.as_ref();
        let Some(lease) = self.leases.load_for_report(lease_id, runner_id).await? else {
            return Err(lease_not_found());
        };

        let lease_key = Uuid7::parse(lease_id).map_err(|_malformed| lease_not_found())?;
        let tool_calls = stored_trace(&lease, request);
        let pricing = Pricing::load(
            self.leases.pool(),
            posture_of(&lease),
            &lease.provider,
            &lease.model,
            lease_id,
        )
        .await;
        // The final slice passes the run's real counts even when the catalogue
        // could not be read: no later slice can charge held tokens, so they
        // land on the line as unpriced, visibly, rather than vanishing.
        let cumulative = Cumulative::reported(
            request.input_tokens,
            request.cached_input_tokens,
            request.output_tokens,
        );
        let report = terminal(
            Subject {
                id: lease_id,
                key: &lease_key,
                runner: runner_id,
                row: &lease,
            },
            cumulative,
            &pricing,
            request,
            tool_calls.as_deref(),
            now,
        );

        match self.leases.commit_report(report).await? {
            Committed::Fenced => Err(stale_fence()),
            Committed::AlreadySettled => {
                self.acknowledge_again(&lease, lease_id).await;
                Ok(reconciled(lease, Nanos::ZERO, true))
            }
            Committed::Settled {
                settlement,
                closed,
                owed,
            } => {
                let charged = usage::charged(settlement.as_deref());
                if let Some(settled) = settlement.as_deref() {
                    self.settle_usage(settled).await;
                }
                self.release_usage(&lease_key, release_of(request)).await;
                self.announce(runner_id, request, &lease, closed, charged, now)
                    .await;
                if let Some(owing) = owed {
                    self.queue_owed_answer(&owing, &lease, request.response_text.as_ref())
                        .await;
                }
                Ok(reconciled(lease, charged, false))
            }
        }
    }
}

/// Why a reported lease's hold is released.
fn release_of(request: &ReportRequest<'_>) -> Release {
    if Verdict::of(
        request.outcome,
        request.failure_reason,
        request.failure_detail.as_ref(),
    )
    .succeeded()
    {
        Release::Completed
    } else {
        Release::Failed
    }
}

/// The lease a report is about, by its three names and its row.
#[derive(Clone, Copy)]
struct Subject<'a> {
    id: &'a str,
    key: &'a Uuid7,
    runner: &'a Uuid7,
    row: &'a Reported,
}

/// Everything the report's transaction needs, assembled from the wire request.
///
/// Separate from [`Plane::report`] so the verb reads as its four steps. The
/// two `try_from` saturations are the request's own numbers: a runner reports
/// its counts and nothing upstream bounds them, so a value past `i64` is
/// clamped rather than refused — the run happened either way, and refusing it
/// here would lose the answer over a telemetry field.
fn terminal<'a>(
    lease: Subject<'a>,
    cumulative: Cumulative,
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
        cumulative,
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
        last_event_id: request.checkpoint.last_event_id.as_ref(),
        last_response: request.checkpoint.last_response.as_ref(),
        held_until: request.held_until,
        now,
    }
}

/// The run's tool trace as the event row stores it, fenced to `lease`.
fn stored_trace(lease: &Reported, request: &ReportRequest<'_>) -> Option<String> {
    let owner = TraceOwner {
        fleet_id: lease.fleet_id.as_str(),
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
fn reconciled(lease: Reported, charged: Nanos, repeated: bool) -> Reconciled {
    Reconciled {
        charged,
        fleet_id: lease.fleet_id,
        workspace_id: lease.workspace_id,
        tenant_id: lease.tenant_id,
        event_id: lease.event_id,
        posture: lease.posture,
        provider: lease.provider,
        model: lease.model,
        repeated,
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
fn posture_of(lease: &Reported) -> Posture {
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

mod steps;

#[cfg(test)]
mod tests;
