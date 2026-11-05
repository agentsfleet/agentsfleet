//! The lease path's one call to its biller: the first hold of a lease issue
//! just committed.
//!
//! A run and what it cost commit together: renewals and reports log their
//! usage lines inside the lease's transaction and return. They never settle or
//! release; the billing relay moves that money from the logged lines through
//! the [`Biller`](afd_billing::biller::Biller) (billing §11), so a biller in
//! another process never joins, or delays, a lease transaction.

use afd_billing::Nanos;
use afd_billing::biller::{Account, Hold, Reservation, WorkKey};
use afd_core::id::Uuid7;

use crate::lease::pull::Plane;

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

}
