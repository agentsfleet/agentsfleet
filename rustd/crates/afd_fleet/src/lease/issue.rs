//! Writing the lease: the durable record that a runner owns a fleet's work.
//!
//! The last write of the lease verb, and the one that makes everything before
//! it observable. Until this row exists a claim is a slot reservation nobody
//! can report against — which is also why the reclaim path is unreachable
//! without it: reclaim looks for a still-`active` lease row, and only this
//! writes one.
//!
//! # The meter reset rides the row
//!
//! A FRESH lease resets the per-fleet metering cursor in the statement that
//! writes the row, so no lease can land over a cursor left from a previous
//! run: the renewal CTE reads it for each slice's delta, and a stale one
//! over-charges the first renewal. A RECLAIM must not reset — the re-leased
//! run meters forward from where the dead holder stopped, which is the whole
//! reason the cursor survives a claim.

use afd_core::id::Uuid7;
use afd_observability::metrics::label::fleet::DeliveryStage;
use afd_observability::producers;
use afd_runner::capacity::choice;
use afd_wire::lease::LeaseKind;
use jiff::Timestamp;
use sqlx::Connection as _;

use crate::error::{Result, query};
use crate::lease::envelope::Acquired;
use crate::lease::sql;
use crate::lease::store::Leases;

/// Statement name, for the context a query failure carries.
const CONTEXT_ISSUE: &str = "lease issue";

/// What the gates resolved, and what the row records about the money.
///
/// Carried in rather than resolved here: a FRESH lease bills and then issues,
/// so the key it billed is the key it delivers — one resolution, no rotation
/// between the two. A RECLAIM never bills at all and carries the dead holder's
/// values forward instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Billed<'a> {
    /// The tenant whose wallet answers for this run.
    pub tenant_id: &'a Uuid7,
    /// The billing posture, as its wire spelling.
    pub posture: &'a str,
    /// The provider resolved at billing.
    ///
    /// Empty on a reclaim, which has no billing pass of its own. Stored beside
    /// the model so the renew gate and the report settle can key the rate row
    /// by `(provider, model)` without re-resolving.
    pub provider: &'a str,
    /// The model resolved at billing.
    pub model: &'a str,
}

/// The issued lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issued {
    /// The lease's durable identifier, which the runner reports against.
    pub lease_id: Uuid7,
}

impl Leases {
    /// Write the lease row for `acquired`, and the audit row that explains it.
    ///
    /// One statement lands the lease, its `lease_acquired` event, the runner's
    /// lifetime tally and a fresh lease's meter reset, so an observer can never
    /// see a lease with no audit row, a tally that has drifted from the rows it
    /// counts, or a fresh lease metering from a previous run's cursor.
    ///
    /// `None` when the claim that earned `acquired` is no longer this pass's:
    /// its fence was superseded or it lapsed, so another runner may already
    /// hold the same entry. Nothing is written, and the claim is not this
    /// pass's to release.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and an entropy source that
    /// could not produce an identifier.
    pub async fn issue(
        &self,
        runner_id: &Uuid7,
        acquired: &Acquired,
        billed: Billed<'_>,
        now: Timestamp,
    ) -> Result<Option<Issued>> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_ISSUE))?;
        let Some(capacity) = afd_runner::capacity::lock(&mut transaction, runner_id).await? else {
            return Ok(None);
        };
        if !capacity.can_claim()
            || acquired.leased_until <= capacity.now()
            || self
                .ticket()
                .is_some_and(|ticket| !ticket.permits(capacity, LeaseKind::Fleet))
        {
            return Ok(None);
        }
        let (lease_id, event_row_id) = self.mint(now)?;
        let written = sql::lease::LeaseRow {
            lease_id: &lease_id,
            runner_id,
            fleet_id: &acquired.fleet_id,
            workspace_id: &acquired.workspace_id,
            tenant_id: billed.tenant_id,
            event_id: &acquired.event_id,
            receipt: acquired.receipt.as_str(),
            event_created_at: acquired.event_created_at,
            posture: billed.posture,
            provider: billed.provider,
            model: billed.model,
            fencing_token: acquired.fence.as_i64(),
            status: sql::LEASE_STATUS_ACTIVE,
            checked_at: capacity.now(),
            event_row_id: &event_row_id,
            kind: acquired.kind.as_str(),
        }
        .bind()
        .execute(&mut *transaction)
        .await
        .map_err(query(CONTEXT_ISSUE))?;
        // The tally arm is the statement's own row count, and it writes
        // exactly when the fenced lease did. The choice record cannot miss
        // here — the host row is locked since `capacity` read its version —
        // and is checked beside the fence the way the authoring claim checks it.
        if written.rows_affected() == 0
            || !choice::record(&mut transaction, runner_id, capacity, LeaseKind::Fleet).await?
        {
            return Ok(None);
        }
        transaction.commit().await.map_err(query(CONTEXT_ISSUE))?;

        record_issued(&lease_id, runner_id, acquired);
        Ok(Some(Issued { lease_id }))
    }

    /// Draw the independent lease and audit identities.
    fn mint(&self, now: Timestamp) -> Result<(Uuid7, Uuid7)> {
        let lease_id = self.entropy().uuid7(now)?;
        let event_row_id = self.entropy().uuid7(now)?;
        Ok((lease_id, event_row_id))
    }
}

fn record_issued(lease_id: &Uuid7, runner_id: &Uuid7, acquired: &Acquired) {
    producers::fleet::delivery_stage(
        DeliveryStage::EventToLease,
        afd_core::clock::elapsed(acquired.event_created_at, afd_core::clock::now()),
    );

    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let lease = lease_id.as_str();
    let fleet = acquired.fleet_id.as_str();
    let runner = runner_id.as_str();
    let event_id = acquired.event_id.as_str();
    let fence = acquired.fence.as_i64();
    let kind = acquired.kind.as_str();
    tracing::debug!(
        event = EVENT_LEASE_ISSUED,
        lease_id = lease,
        fleet_id = fleet,
        runner_id = runner,
        agentsfleet_event_id = event_id,
        fencing_token = fence,
        kind,
        "a runner now owns this fleet's work"
    );
}

/// A lease was written and handed to a runner.
///
/// `LOGGING_STANDARD.md` §3 `event` value. Dashboards match on this spelling,
/// so changing it is an operator-visible change.
const EVENT_LEASE_ISSUED: &str = "lease_issued";
