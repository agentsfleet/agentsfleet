//! Writing the lease: the durable record that a runner owns a fleet's work.
//!
//! The last write of the lease verb, and the one that makes everything before
//! it observable. Until this row exists a claim is a slot reservation nobody
//! can report against — which is also why the reclaim path is unreachable
//! without it: reclaim looks for a still-`active` lease row, and only this
//! writes one.
//!
//! # Fail-closed on the meter reset
//!
//! A FRESH lease resets the per-fleet metering cursor before the row is
//! written, and a failed reset fails the issue. That direction is deliberate:
//! the renewal CTE reads the cursor for each slice's delta, so issuing against
//! a cursor left over from a previous run over-charges the first renewal. A
//! lease not issued costs one poll; a lease issued on a stale cursor costs
//! money and nobody notices.
//!
//! A RECLAIM must not reset — the re-leased run meters forward from where the
//! dead holder stopped, which is the whole reason the cursor survives a claim.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};

use crate::error::{Result, query};
use crate::lease::envelope::{Acquired, Kind};
use crate::lease::store::Leases;
use crate::sql;

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
    /// One statement lands the lease, its `lease_acquired` event, and the
    /// runner's lifetime tally, so an observer can never see a lease with no
    /// audit row or a tally that has drifted from the rows it counts.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, an entropy source that could
    /// not produce an identifier, and — deliberately — a failed meter reset on
    /// a fresh lease.
    pub async fn issue(
        &self,
        runner_id: &Uuid7,
        acquired: &Acquired,
        billed: Billed<'_>,
        now: UnixMillis,
    ) -> Result<Issued> {
        // Before the row, and fail-closed: see the module documentation.
        if acquired.kind == Kind::Fresh {
            self.reset_meters(&acquired.fleet_id, now).await?;
        }

        let (lease_id, event_row_id) = self.mint(now)?;
        let mut connection = self.pool().acquire().await?;
        sql::lease::LeaseRow {
            lease_id: &lease_id,
            runner_id,
            fleet_id: &acquired.fleet_id,
            workspace_id: &acquired.workspace_id,
            tenant_id: billed.tenant_id,
            event_id: &acquired.event_id,
            actor: &acquired.actor,
            event_type: &acquired.event_type,
            event_created_at: acquired.event_created_at.as_millis(),
            posture: billed.posture,
            provider: billed.provider,
            model: billed.model,
            fencing_token: acquired.fence.as_i64(),
            leased_until: acquired.leased_until.as_millis(),
            status: sql::LEASE_STATUS_ACTIVE,
            now,
            event_row_id: &event_row_id,
            kind: acquired.kind.as_str(),
        }
        .bind()
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT_ISSUE))?;

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
        Ok(Issued { lease_id })
    }

    /// Draws the two identifiers a lease row needs.
    ///
    /// Split out because it is the only part that touches entropy, and because
    /// two consecutive fallible draws inside the write would push
    /// [`Leases::issue`] past the function-length line for no gain.
    fn mint(&self, now: UnixMillis) -> Result<(Uuid7, Uuid7)> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy().fill(&mut bytes)?;
        let lease_id = Uuid7::encode(now, bytes)?;
        self.entropy().fill(&mut bytes)?;
        let event_row_id = Uuid7::encode(now, bytes)?;
        Ok((lease_id, event_row_id))
    }
}

/// A lease was written and handed to a runner.
///
/// `LOGGING_STANDARD.md` §3 `event` value, spelled as `service.zig` spells it
/// so a dashboard built against the Zig daemon keeps matching after cutover.
const EVENT_LEASE_ISSUED: &str = "lease_issued";
