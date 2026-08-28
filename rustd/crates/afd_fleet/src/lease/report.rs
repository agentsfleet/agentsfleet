//! The report verb: one runner's terminal result, from fence to acknowledgement.
//!
//! Two halves with a commit between them. Everything before
//! [`Leases::claim_and_settle`] can still refuse the report; nothing after it
//! can, because by then the lease is flipped and the tenant is charged.
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
//! where nothing can take it away.
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
use crate::lease::pull::Plane;
use crate::lease::settle::{Reported, Settled};
use crate::lease::verdict::{Terminal, Verdict};
use afd_billing::rates::Posture;
use afd_billing::{Cumulative, Nanos};

/// The scoped event a finalize step is logged under when it does not land.
const EVENT_FINALIZE_FAILED: &str = "report_finalize_step_failed";

/// A settled report was written.
const EVENT_SETTLED: &str = "report_settled";

impl Plane {
    /// Record one runner's terminal result for a lease it holds.
    ///
    /// Answers what the final slice drained, so the caller can meter it. The
    /// answer is a [`Nanos`] rather than a metric emitted here for the reason
    /// the receive debit gives.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's with
    /// [`lease_not_found`](crate::error), and a holder the fleet has superseded
    /// with [`stale_fence`](crate::error) — neither writes anything. Also
    /// reports a datastore that would not answer, in which case the lease is
    /// left `active` for the runner to re-report against.
    pub async fn report(
        &self,
        runner_id: &Uuid7,
        request: &ReportRequest<'_>,
        now: UnixMillis,
    ) -> Result<Nanos> {
        let lease_id = request.lease_id.as_ref();
        let Some(lease) = self.leases.load_for_report(lease_id, runner_id).await? else {
            return Err(lease_not_found());
        };

        let verdict = Verdict::of(
            request.outcome,
            request.failure_reason,
            request.failure_detail.as_ref(),
        );
        let charged = match self
            .settle(runner_id, lease_id, &lease, request, verdict, now)
            .await?
        {
            Settled::Claimed(nanos) => nanos,
            Settled::Fenced => return Err(stale_fence()),
        };

        // Hoisted for the `log` bridge's duplicated field expressions.
        let fleet = lease.fleet_id.as_str();
        let event = lease.event_id.as_str();
        let nanos = charged.as_i64();
        tracing::debug!(
            fleet_id = fleet,
            agentsfleet_event_id = event,
            lease_id,
            nanos,
            event = EVENT_SETTLED,
            "the report won its fence and its final slice was charged"
        );

        self.finalize(runner_id, lease_id, &lease, request, verdict, now)
            .await;
        Ok(charged)
    }

    /// Price the final slice and spend the fence on it.
    ///
    /// The rate resolution is fail-OPEN and the posture is stated here rather
    /// than inside the resolver: a datastore fault while pricing must not
    /// refuse a report whose run has already happened, because the run cannot
    /// be un-run and the alternative is charging nothing at all. So a fault
    /// meters run-fee-only, exactly as `buildMeterInputs` does — the difference
    /// is that [`afd_billing::Accounts::meter`] hands the decision UP to here,
    /// where it is one line a reader can find, instead of absorbing it eight
    /// frames down.
    async fn settle(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        lease: &Reported,
        request: &ReportRequest<'_>,
        verdict: Verdict<'_>,
        now: UnixMillis,
    ) -> Result<Settled> {
        let posture = posture_of(lease);
        let cumulative = Cumulative::reported(
            request.input_tokens,
            request.cached_input_tokens,
            request.output_tokens,
        );
        let meter = match self
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
                    lease_id,
                    reason,
                    event = "report_rates_unverified_run_fee_only",
                    "the catalogue could not be read; the final slice meters runtime only"
                );
                self.accounts.run_fee_meter(cumulative)
            }
        };
        self.leases
            .claim_and_settle(lease_id, runner_id, meter, verdict.succeeded(), now)
            .await
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

impl Plane {
    /// The five durable writes a won report still owes, none of them fatal.
    ///
    /// Every step is attempted independently and logged if it does not land.
    /// Nothing here can fail the verb: the claim and the settle have already
    /// committed, so an error now would ask the runner to retry a report whose
    /// money cannot be charged twice — and the retry would be fenced anyway,
    /// turning a partial success into a permanent 409.
    ///
    /// The order is the Zig's `finalize`: end the work, checkpoint where it
    /// resumes, acknowledge the stream entry, free the slot, then close the
    /// lease's own history. The slot release is late deliberately — it makes
    /// the fleet's next event claimable, and doing that before the event row is
    /// terminal would let a fresh lease race a half-written finalize.
    async fn finalize(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        lease: &Reported,
        request: &ReportRequest<'_>,
        verdict: Verdict<'_>,
        now: UnixMillis,
    ) {
        let event_id = lease.event_id.as_str();
        let wall_ms = i64::try_from(request.telemetry.wall_ms).unwrap_or(i64::MAX);
        let tokens = i64::try_from(request.tokens).unwrap_or(i64::MAX);

        let outcome = Terminal {
            verdict,
            response_text: request.response_text.as_ref(),
            tokens,
            wall_ms,
        };
        step(
            "mark_terminal",
            lease,
            lease_id,
            self.leases
                .mark_terminal(&lease.fleet_id, event_id, outcome, now)
                .await,
        );
        step(
            "checkpoint",
            lease,
            lease_id,
            self.leases
                .checkpoint(
                    &lease.fleet_id,
                    request.checkpoint.last_event_id.as_ref(),
                    request.checkpoint.last_response.as_ref(),
                    now,
                )
                .await,
        );
        step(
            "acknowledge",
            lease,
            lease_id,
            self.leases.acknowledge(&lease.fleet_id, event_id).await,
        );
        step(
            "release_slot",
            lease,
            lease_id,
            self.leases
                .release_slot(&lease.fleet_id, lease.fence, now)
                .await,
        );
        step(
            "record_released",
            lease,
            lease_id,
            self.leases
                .record_released(runner_id, lease_id, &lease.fleet_id, event_id, now)
                .await,
        );
    }
}

/// Log one finalize step that did not land, and carry on.
///
/// Takes the already-awaited outcome rather than a future, so each call site
/// above reads as the step it names. The `step` label is what an operator greps
/// to find WHICH of the five is failing — a single `finalize_failed` line would
/// say only that something after the money did not work, which is the least
/// actionable version of this warning.
fn step(step: &'static str, lease: &Reported, lease_id: &str, outcome: Result<()>) {
    let Err(failure) = outcome else {
        return;
    };
    let fleet = lease.fleet_id.as_str();
    let event = lease.event_id.as_str();
    let reason = failure.to_string();
    let code = failure.code().as_str();
    tracing::warn!(
        error_code = code,
        fleet_id = fleet,
        agentsfleet_event_id = event,
        lease_id,
        step,
        reason,
        event = EVENT_FINALIZE_FAILED,
        "a post-settle finalize step did not land; the report still stands"
    );
}
