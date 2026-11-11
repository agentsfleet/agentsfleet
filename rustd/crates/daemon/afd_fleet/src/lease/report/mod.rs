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
//! No activity publish and no outbound wake inside the settle. An owed
//! answer's obligation row does commit there, through
//! [`crate::lease::obligation`]; the publish and the wake run after the
//! commit, best effort, and a failure is logged, never returned.
//! No OTLP spans either — the drained amount comes back as a VALUE, the way
//! [`afd_billing::Accounts::debit_receive`] answers one, and the caller
//! attaches the instrument. An exporter fused into the money path would be one
//! a report could not settle without.

use af_core::id::Uuid7;
use af_wire::report::ReportRequest;
use jiff::Timestamp;

use crate::error::{Result, lease_not_found};
use crate::lease::Reported;
use crate::lease::pull::LeaseService;
use crate::lease::usage::Pricing;

use assembly::{Subject, posture_of, stored_trace, terminal};
pub use result::Reconciled;
pub use tokens::RunTokens;
mod assembly;
mod finalize;
mod observations;
mod result;
pub use observations::observe;

impl LeaseService {
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
        self.report_with(runner_id, request, now, None).await
    }

    /// [`Self::report`] over the lease's facts a stream session kept from the
    /// offer, so the report reads no lease row (M245 §9, blocker 6). `None`
    /// reads them, as a session rebuilt from the database must. The facts
    /// never change after issue, and the report statement still checks the
    /// fence.
    ///
    /// # Errors
    /// As [`Self::report`].
    pub async fn report_with(
        &self,
        runner_id: &Uuid7,
        request: &ReportRequest<'_>,
        now: Timestamp,
        kept: Option<Reported>,
    ) -> Result<Reconciled> {
        let lease_id = &crate::error::wire_lease(request.lease_id.as_ref())?;
        let lease = match kept {
            Some(lease) => lease,
            None => match self.leases.load_for_report(lease_id, runner_id).await? {
                Some(lease) => lease,
                None => return Err(lease_not_found()),
            },
        };

        let lease_key = lease_id.clone();
        let task_plan = self
            .prepare_task_report(runner_id, &lease_key, request)
            .await?;
        let task_settlement = match &task_plan {
            Some(plan) => Some(plan.settlement().await?),
            None => None,
        };
        let tool_calls = stored_trace(&lease, request);
        // The book cached at the version this report read beside the lease:
        // a warm replica reads no price, and the statement guards on the
        // version (M245 §9).
        let pricing = match lease.catalogue_version {
            Some(version) => {
                self.leases
                    .prices()
                    .get_or_load(
                        self.leases.pool(),
                        posture_of(&lease),
                        &lease.provider,
                        &lease.model,
                        Some(version),
                        lease_id.as_str(),
                    )
                    .await
            }
            None => std::sync::Arc::new(
                Pricing::load(
                    self.leases.pool(),
                    posture_of(&lease),
                    &lease.provider,
                    &lease.model,
                    lease_id.as_str(),
                )
                .await,
            ),
        };
        let last_event = Uuid7::parse(request.checkpoint.last_event_id.as_ref())?;
        let mut report = terminal(
            Subject {
                id: lease_id,
                last_event: &last_event,
                key: &lease_key,
                runner: runner_id,
                row: &lease,
            },
            &pricing,
            request,
            tool_calls.as_deref(),
            now,
        );

        report.task_settlement = task_settlement;
        // Boxed: the capture's state would otherwise ride inside every report
        // future, past the large-future bound, for a step most reports skip.
        Box::pin(self.capture_reported(runner_id, &lease, request, now)).await;
        let committed = self.leases.commit_report(report).await?;
        self.finalize_report(
            lease,
            committed,
            task_plan.as_ref(),
            finalize::Context {
                runner_id,
                request,
                lease_id,
                now,
            },
        )
        .await
    }
}

mod memory;
mod steps;
mod timing;
mod tokens;

#[cfg(test)]
mod tests;
