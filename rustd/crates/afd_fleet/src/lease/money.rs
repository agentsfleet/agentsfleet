//! The lease path's calls to its biller, made after the work's transaction
//! commits.
//!
//! A run and what it cost commit together: the usage line is logged inside the
//! lease's transaction. Money moves afterwards, through the
//! [`Biller`](afd_billing::biller::Biller), because a queue or a third-party
//! biller can never join our transaction (billing §11). A call that fails here
//! has lost nothing: the line stays logged and unsettled, and the billing
//! sweeper settles it, so neither function returns an error to its verb.

use afd_billing::Nanos;
use afd_billing::biller::{Account, Hold, Release, Reservation, Settlement, WorkKey};
use afd_core::id::Uuid7;

use crate::lease::pull::Plane;

/// Logged when a settlement after commit did not land; the sweeper retries it.
const EVENT_SETTLE_DEFERRED: &str = "lease_settle_deferred";

/// Logged when a hold release after a report did not land; the sweeper
/// retries it.
const EVENT_RELEASE_DEFERRED: &str = "lease_release_deferred";

/// Logged when a freshly issued lease's first hold was not placed.
const EVENT_HOLD_UNPLACED: &str = "lease_hold_unplaced";

/// What [`EVENT_HOLD_UNPLACED`] says, whichever way the hold was missed.
const MESSAGE_HOLD_UNPLACED: &str = "the lease is issued unheld; its first renewal reserves again";

impl Plane {
    /// Place a freshly issued lease's first hold: the floor its balance gate
    /// admitted against.
    ///
    /// After the lease commits, because the hold names the lease. A balance
    /// that moved between the gate and here, or a biller that could not
    /// answer, leaves the lease running unheld; its first renewal raises the
    /// hold and refuses there if the balance cannot fund it.
    pub(crate) async fn hold_issued(
        &self,
        lease_id: &Uuid7,
        tenant: &Uuid7,
        workspace: &Uuid7,
        floor: Nanos,
    ) {
        let ask = Reservation {
            work: WorkKey::FleetLease(lease_id.clone()),
            payer: Account {
                tenant: tenant.clone(),
                workspace: workspace.clone(),
            },
            ceiling: floor,
        };
        let work_id = lease_id.as_str();
        match self.biller.reserve(&ask).await {
            Ok(Hold::Held { .. } | Hold::Unconfirmed) => {}
            Ok(held) => tracing::warn!(
                work_id,
                hold = ?held,
                event = EVENT_HOLD_UNPLACED,
                MESSAGE_HOLD_UNPLACED
            ),
            Err(fault) => {
                let error_code = fault.code().as_str();
                tracing::warn!(
                    error = %fault,
                    error_code,
                    work_id,
                    event = EVENT_HOLD_UNPLACED,
                    MESSAGE_HOLD_UNPLACED
                );
            }
        }
    }

    /// Charge `settlement`'s lines through the biller.
    pub(crate) async fn settle_usage(&self, settlement: &Settlement) {
        if let Err(fault) = self.biller.settle(settlement).await {
            let work_id = settlement.work.id().as_str();
            let error_code = fault.code().as_str();
            tracing::warn!(
                error = %fault,
                error_code,
                work_id,
                event = EVENT_SETTLE_DEFERRED,
                "the usage line is logged but not yet charged; the billing sweeper settles it"
            );
        }
    }

    /// Free what is left of `lease_id`'s hold, charging anything still
    /// unsettled first.
    pub(crate) async fn release_usage(&self, lease_id: &Uuid7, why: Release) {
        let work = WorkKey::FleetLease(lease_id.clone());
        if let Err(fault) = self.biller.release(&work, why).await {
            let work_id = lease_id.as_str();
            let error_code = fault.code().as_str();
            tracing::warn!(
                error = %fault,
                error_code,
                work_id,
                event = EVENT_RELEASE_DEFERRED,
                "the lease's hold is still open; the billing sweeper releases it"
            );
        }
    }
}
