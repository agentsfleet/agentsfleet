//! The narrative log's row: how a delivery relates to it, and the gate ending.
//!
//! The claim statement opens the row (`sql/lease/claim_admission_head.sql`):
//! the first thing the lease verb writes, before any gate and before any
//! money. `data_flow.md` §C numbers it 1 for that reason — everything after it
//! either amends this row or refers to it, and the reclaim path's INNER JOIN
//! reads it to recover an event body without keeping a second copy on the
//! lease. It opens before the gates because a gate refusal has to have a row
//! to mark `gate_blocked`.
//!
//! # The refusal write lives here
//!
//! [`Leases::block`] is the other end of the same row. The guard that makes
//! the ending safe is a statement about the OPENING — the terminal update
//! fires only on a row still in the status the claim wrote — so the
//! [`Delivery`] the claim answers and the ending that reads it sit together.

use jiff::Timestamp;
use sqlx::Connection as _;

use afd_api_wire::tail::FleetCounters;
use afd_events::Closed;

use crate::error::{Result, query};
use crate::lease::admit::Refusal;
use crate::lease::envelope::Acquired;
use crate::lease::store::Leases;

/// Statement name, for the context a refusal's query failure carries.
const CONTEXT_BLOCKED: &str = "fleet event gate blocked";

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
    /// The row already existed and is still OPEN, so an earlier delivery paid
    /// for it and this one may carry on executing it.
    Repeat,
    /// The row already existed and has already FINISHED.
    ///
    /// Dimension 7.4. A redelivery of an event that already ran must be
    /// acknowledged and dropped, not executed: the run happened, the tenant
    /// paid for it, and the answer is already owed or delivered. Running it
    /// again spends a provider's money a second time and posts a second answer
    /// into a thread a person is reading.
    ///
    /// A third arm rather than a `bool` beside [`Self::Repeat`] for the reason
    /// the enum exists at all — every `match` on this type now has to say what
    /// it does with a finished event, and the compiler is what asks.
    Terminal,
}

/// What opening the narrative log answered: whether this was the first
/// delivery, and where the fleet's counters stand if it was.
///
/// The counters ride here because the insert is what moves them — the
/// trigger fires on it, and a `RETURNING` cannot see the trigger's write —
/// so the read follows the insert on the same connection, where the write is
/// already visible and no second acquire is paid. `None` on a redelivery,
/// whose frame was published by the delivery that wrote the row, and on a
/// first delivery whose read did not answer: the frame then goes out without
/// its figures, never with zeros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Received {
    /// Whether this delivery wrote the row.
    pub delivery: Delivery,
    /// The fleet's counters after the row landed, when this delivery wrote it.
    pub counters: Option<FleetCounters>,
    /// The row's stored `created_at`: a steer's admission instant, otherwise
    /// the `now` this delivery passed. The received frame carries it, so the
    /// tail announces the instant the history read lists the row under.
    pub opened_at: Timestamp,
}

/// Whether the refusal moved a row, and the row it moved.
///
/// A named type for the reason [`Delivery`] is one: the answer looks like
/// success or failure and is neither. No row means the event was ALREADY
/// terminal, which happens when a refused delivery's acknowledgement was lost
/// and the producer re-delivered it. The refusal still stands and the
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
    /// An earlier pass already did.
    Already,
}

impl Leases {
    /// End `event_id` at a gate, naming what refused it.
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
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_BLOCKED))?;
        if !self
            .lock_admission_on(
                &mut transaction,
                &acquired.fleet_id,
                &acquired.event_id,
                acquired.fence,
            )
            .await?
        {
            return Err(crate::error::stale_fence());
        }
        let closed = sqlx::query(afd_events::sql::UPDATE_FLEET_EVENT_FAILURE)
            .bind(&acquired.fleet_id)
            .bind(&acquired.event_id)
            .bind(af_core::event::status::GATE_BLOCKED)
            .bind(refusal.label)
            .bind(af_core::event::status::RECEIVED)
            // Empty stores as SQL NULL through the statement's `NULLIF`, which
            // is what keeps the row shape identical for the refusals that
            // carry no operator-readable instruction.
            .bind(refusal.detail)
            .bind(afd_api_wire::approval::status::PENDING)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_BLOCKED))?;

        self.settle_terminal_on(&mut transaction, acquired).await?;
        let ended = match closed {
            Some(row) => Ended::Now(Box::new(Closed::read(&row)?)),
            None => Ended::Already,
        };
        transaction.commit().await.map_err(query(CONTEXT_BLOCKED))?;
        Ok(ended)
    }
}
