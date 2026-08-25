//! Hot-path write one: the narrative log opens.
//!
//! The first thing the lease verb writes, before any gate and before any
//! money. `data_flow.md` §C numbers it 1 for that reason — everything after it
//! either amends this row or refers to it, and the reclaim path's INNER JOIN
//! reads it to recover an event body without keeping a second copy on the
//! lease.
//!
//! # Why the caller is not `issue`
//!
//! In the Zig daemon this write opens `runBilling`, ahead of the balance and
//! approval gates, because a gate refusal has to have a row to mark
//! `gate_blocked`. Porting it into the issue step instead would move it AFTER
//! those gates and leave a refused event with nothing to record its refusal
//! on. So it stays a verb of its own, and the gate pass calls it first — the
//! same position it holds upstream.

use afd_core::clock::UnixMillis;

use crate::error::{Result, query};
use crate::lease::envelope::Acquired;
use crate::lease::store::Leases;
use crate::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_RECEIVED: &str = "fleet event received";

/// Whether this delivery was the first.
///
/// A named type rather than a `bool`, because the answer decides whether the
/// tenant is CHARGED: the receive debit fires on a first delivery and must not
/// on a redelivery, since the balance debit is not replay-guarded. A bare bool
/// at that call site is one `!` away from billing every re-poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// This event had no row; the caller owes it a receive debit.
    First,
    /// The row already existed, so an earlier delivery already paid.
    Repeat,
}

impl Leases {
    /// Open the narrative log for `acquired`, if it is not already open.
    ///
    /// Answers [`Delivery::Repeat`] when the row was already there — the
    /// `ON CONFLICT` arm — which is how a re-leased or re-polled event is told
    /// apart from a new one without a second read.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. The workspace is already a
    /// [`Uuid7`](afd_core::id::Uuid7) by the time it arrives — the envelope
    /// parsed it — so there is nothing left here to validate.
    pub async fn record_received(&self, acquired: &Acquired, now: UnixMillis) -> Result<Delivery> {
        let mut connection = self.pool().acquire().await?;
        let landed = sqlx::query(sql::event::INSERT_FLEET_EVENT)
            .bind(acquired.fleet_id.as_str())
            .bind(&acquired.event_id)
            .bind(acquired.workspace_id.as_str())
            .bind(&acquired.actor)
            .bind(&acquired.event_type)
            .bind(&acquired.request_json)
            .bind(Option::<&str>::None)
            .bind(now.as_millis())
            .bind(sql::event::EVENT_STATUS_RECEIVED)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RECEIVED))?;

        // Zero rows is the `ON CONFLICT DO NOTHING` arm: the row was already
        // there, so somebody has already paid for this event.
        Ok(if landed.rows_affected() == 0 {
            Delivery::Repeat
        } else {
            Delivery::First
        })
    }
}
