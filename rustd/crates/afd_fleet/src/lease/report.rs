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
//! loses money on the cap path. A run that reaches `MAX_RUNTIME_MS` is racing
//! the reclaim sweep, and the sweep bumps `fencing_seq`. Settle first and the
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
//! No activity publish and no connector outbound hand-off: both are §4's, and
//! both are pure fan-out that writes no durable row this milestone's parity is
//! measured on. No OTLP spans either — the drained amount comes back as a
//! VALUE, the way [`afd_billing::Accounts::debit_receive`] answers one, and
//! M181 §5 attaches the instrument. Fusing an exporter into the money path is
//! what makes `service_billing.zig` unable to run without one configured.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::report::ReportRequest;

use crate::error::{Result, lease_not_found, stale_fence};
use crate::lease::commit::{Committed, TerminalReport};
use crate::lease::pull::Plane;
use crate::lease::settle::Reported;
use crate::lease::verdict::{Terminal, Verdict};
use afd_billing::rates::Posture;
use afd_billing::{Cumulative, Meter, Nanos};

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
/// belongs on the money path — fusing an exporter into it is what leaves
/// `service_billing.zig` unable to run without one configured — so the money
/// path answers with the facts and the handler decides what to say about them.
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
    /// Answers what the final slice drained, so the caller can meter it, and
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
    /// charged.
    pub async fn report(
        &self,
        runner_id: &Uuid7,
        request: &ReportRequest<'_>,
        now: UnixMillis,
    ) -> Result<Reconciled> {
        let lease_id = request.lease_id.as_ref();
        let Some(lease) = self.leases.load_for_report(lease_id, runner_id).await? else {
            return Err(lease_not_found());
        };

        let verdict = Verdict::of(
            request.outcome,
            request.failure_reason,
            request.failure_detail.as_ref(),
        );
        let meter = self.price_final_slice(&lease, request).await;
        let report = terminal(lease_id, runner_id, &lease, meter, verdict, request, now);

        match self.leases.commit_report(report).await? {
            Committed::Fenced => Err(stale_fence()),
            Committed::AlreadySettled => {
                self.acknowledge_again(&lease, lease_id).await;
                Ok(reconciled(lease, Nanos::ZERO, true))
            }
            Committed::Settled {
                charged,
                closed,
                owed,
            } => {
                self.announce(runner_id, lease_id, &lease, closed, charged, now)
                    .await;
                if let Some(obligation) = owed {
                    self.queue_owed_answer(
                        &obligation,
                        &lease,
                        request.response_text.as_ref(),
                        now,
                    )
                    .await;
                }
                Ok(reconciled(lease, charged, false))
            }
        }
    }

    /// Price the final slice, fail-OPEN.
    ///
    /// The rate resolution is fail-OPEN and the posture is stated here rather
    /// than inside the resolver: a datastore fault while pricing must not
    /// refuse a report whose run has already happened, because the run cannot
    /// be un-run and the alternative is charging nothing at all. So a fault
    /// meters run-fee-only, exactly as `buildMeterInputs` does — the difference
    /// is that [`afd_billing::Accounts::meter`] hands the decision UP to here,
    /// where it is one line a reader can find, instead of absorbing it eight
    /// frames down.
    async fn price_final_slice(&self, lease: &Reported, request: &ReportRequest<'_>) -> Meter {
        let posture = posture_of(lease);
        let cumulative = Cumulative::reported(
            request.input_tokens,
            request.cached_input_tokens,
            request.output_tokens,
        );
        match self
            .accounts
            .meter(posture, &lease.provider, &lease.model, cumulative)
            .await
        {
            Ok(meter) => meter,
            Err(failure) => {
                let fleet = lease.fleet_id.as_str();
                let reason = failure.to_string();
                tracing::warn!(
                    fleet_id = fleet,
                    lease_id = request.lease_id.as_ref(),
                    reason,
                    event = "report_rates_unverified_run_fee_only",
                    "the catalogue could not be read; the final slice meters runtime only"
                );
                self.accounts.run_fee_meter(cumulative)
            }
        }
    }
}

/// Everything the report's transaction needs, assembled from the wire request.
///
/// Separate from [`Plane::report`] so the verb reads as its four steps. The
/// two `try_from` saturations are the request's own numbers: a runner reports
/// its counts and nothing upstream bounds them, so a value past `i64` is
/// clamped rather than refused — the run happened either way, and refusing it
/// here would lose the answer over a telemetry field.
fn terminal<'a>(
    lease_id: &'a str,
    runner_id: &'a Uuid7,
    lease: &'a Reported,
    meter: Meter,
    verdict: Verdict<'a>,
    request: &'a ReportRequest<'a>,
    now: UnixMillis,
) -> TerminalReport<'a> {
    TerminalReport {
        lease_id,
        runner_id,
        lease,
        meter,
        outcome: Terminal {
            verdict,
            response_text: request.response_text.as_ref(),
            tokens: i64::try_from(request.tokens).unwrap_or(i64::MAX),
            wall_ms: i64::try_from(request.telemetry.wall_ms).unwrap_or(i64::MAX),
        },
        last_event_id: request.checkpoint.last_event_id.as_ref(),
        last_response: request.checkpoint.last_response.as_ref(),
        now,
    }
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

#[path = "report/steps.rs"]
mod steps;

#[cfg(test)]
#[path = "report/tests.rs"]
mod tests;
