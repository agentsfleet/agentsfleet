//! The narrative log's two ends on the lease path: how a claim opened it, and
//! how a refused or already-finished event closes it.
//!
//! The claim statement opens the row (`super::assign`), in the statement that
//! stamps the fence, before any gate and before any money: a gate refusal has
//! to have a row to mark `gate_blocked`. What the claim read back is a
//! [`Delivery`], and this module is where its two non-running readings end.
//!
//! # One statement per ending
//!
//! [`Leases::block`] and [`Leases::settle_terminal`] each lock the slot under
//! the fence the claim stamped, settle the admission and free the slot in one
//! statement; the admission trigger then moves the fleet's readiness to its
//! next admission. A fence that has moved on matches no slot, so a stale ending
//! writes nothing at all.

use afd_events::Closed;

use crate::error::{Result, query};
use crate::lease::admit::Refusal;
use crate::lease::envelope::Acquired;
use crate::lease::sql::queue;
use crate::lease::store::Leases;

/// Statement name, for the context a refusal's query failure carries.
const CONTEXT_BLOCKED: &str = "fleet event gate blocked";

/// Statement name, for the context a terminal settle's failure carries.
const CONTEXT_TERMINAL: &str = "terminal redelivery settle";

/// Whether this delivery was the first.
///
/// A named type rather than a `bool`, because the answer decides whether the
/// tenant is CHARGED: the receive debit fires on a first delivery and must not
/// on a redelivery, since the balance debit is not replay-guarded. A bare bool
/// at that call site is one `!` away from billing every re-poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// The claim wrote this event's row; the caller owes it a receive debit.
    First,
    /// The row already existed and is still OPEN, so an earlier delivery paid
    /// for it and this one may carry on executing it.
    Repeat,
    /// The row already existed and has already FINISHED.
    ///
    /// A redelivery of an event that already ran must be acknowledged and
    /// dropped, not executed: the run happened, the tenant paid for it, and
    /// the answer is already owed or delivered. Running it again spends a
    /// provider's money a second time and posts a second answer into a thread
    /// a person is reading.
    ///
    /// Open means `received` and nothing else: every other stored spelling is
    /// an ending somebody already wrote, a runner's `processed` or
    /// `fleet_error`, or a daemon-side refusal like `gate_blocked`. Asking "is
    /// it still open" rather than listing the endings is what keeps this
    /// correct when a new refusal spelling is added.
    Terminal,
}

/// Whether the refusal moved a row, and the row it moved.
///
/// A named type for the reason [`Delivery`] is one: the answer looks like
/// success or failure and is neither. No row means the event was ALREADY
/// terminal, which happens when a refused delivery's acknowledgement was lost
/// and the producer re-delivered it, or the claim's fence was superseded and
/// the statement wrote nothing. The refusal still stands and the
/// acknowledgement is still owed, so the caller proceeds either way — it just
/// must not report the second pass as though it decided something.
///
/// The pass that DID decide carries the closing: the terminal row and the
/// fleet facts beside it, which the caller announces on the live tail. Boxed
/// so the enum stays the size of a discriminant on the arm that carries
/// nothing — the closing is built once per refused event and read once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    /// This pass wrote the terminal row, and here it is.
    Now(Box<Closed>),
    /// An earlier pass already did, or this pass no longer holds the claim.
    Already,
}

impl Leases {
    /// End `acquired`'s event at a gate, naming what refused it, settle its
    /// admission and free the slot — one statement under the claim's fence.
    ///
    /// Guarded on the row still being `received`, so a terminal row is never
    /// reopened: a request arriving after a `gate_blocked` is a NEW delivery
    /// with its own row, not a resurrection of this one.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a closing this daemon
    /// cannot read. A row that has already ended is [`Ended::Already`], not
    /// an error — see that type.
    pub async fn block(&self, acquired: &Acquired, refusal: Refusal) -> Result<Ended> {
        let mut connection = self.pool().acquire().await?;
        self.tally().statements(1);
        let closed = sqlx::query(queue::END_REFUSED)
            .bind(acquired.fleet_id.as_str())
            .bind(&acquired.event_id)
            .bind(acquired.fence.as_i64())
            .bind(afd_core::event::status::GATE_BLOCKED)
            .bind(refusal.label)
            .bind(afd_core::event::status::RECEIVED)
            // Empty stores as SQL NULL through the statement's `NULLIF`, which
            // is what keeps the row shape identical for the refusals that
            // carry no operator-readable instruction.
            .bind(refusal.detail)
            .bind(afd_api_wire::approval::status::PENDING)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_BLOCKED))?;
        Ok(match closed {
            Some(row) => Ended::Now(Box::new(Closed::read(&row)?)),
            None => Ended::Already,
        })
    }

    /// Settle a redelivered event that already finished, and free its slot,
    /// without running it again — one statement under the claim's fence, so a
    /// stale claimant cannot clear it.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub(crate) async fn settle_terminal(&self, acquired: &Acquired) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        self.tally().statements(1);
        sqlx::query(queue::SETTLE_TERMINAL)
            .bind(acquired.fleet_id.as_str())
            .bind(&acquired.event_id)
            .bind(acquired.fence.as_i64())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_TERMINAL))?;
        Ok(())
    }

    /// Announce the row the claim opened, with the fleet's counters after it.
    ///
    /// The counters ride a read of their own: the event insert's trigger is
    /// what moves them, and a `RETURNING` cannot see a trigger's write. A read
    /// that does not answer sends the frame without its figures, never with
    /// zeros. Only a first delivery announces: a redelivery's watchers already
    /// hold the opening bracket.
    pub async fn publish_opened(&self, acquired: &Acquired) {
        self.tally().statements(1);
        self.tally().commands(1);
        let counters =
            afd_events::fleet_counters_best_effort(self.pool(), acquired.fleet_id.as_str()).await;
        self.publish_received(acquired, acquired.opened_at, counters)
            .await;
    }
}
