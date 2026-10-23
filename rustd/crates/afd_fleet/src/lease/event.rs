//! Hot-path write one: the narrative log opens.
//!
//! The first thing the lease verb writes, before any gate and before any
//! money. `data_flow.md` §C numbers it 1 for that reason — everything after it
//! either amends this row or refers to it, and the reclaim path's INNER JOIN
//! reads it to recover an event body without keeping a second copy on the
//! lease.
//!
//! # The refusal write lives here too
//!
//! [`Leases::block`] is the other end of the same row: this module opens the
//! narrative log and ends it at a gate. They are one file because the guard
//! that makes the ending safe is a statement about the OPENING — the terminal
//! update fires only on a row still in the status this insert wrote — and a
//! reader checking that predicate should find both halves of it together.
//!
//! # Why the caller is not `issue`
//!
//! This write opens the pass in `pull.rs`, ahead of the balance and approval
//! gates, because a gate refusal has to have a row to mark `gate_blocked`.
//! Folded into the issue step it would run AFTER those gates and leave a
//! refused event with nothing to record its refusal on. So it stays a verb of
//! its own, and the gate pass calls it first.

use jiff::Timestamp;
use sqlx::Connection as _;

use afd_api_wire::tail::FleetCounters;
use afd_core::id::Uuid7;
use afd_events::Closed;

use crate::error::{Result, query};
use crate::lease::admit::Refusal;
use crate::lease::envelope::Acquired;
use crate::lease::sql::queue;
use crate::lease::store::Leases;

/// Statement name, for the context a query failure carries.
const CONTEXT_RECEIVED: &str = "fleet event received";

/// Statement name, for the context a refusal's query failure carries.
const CONTEXT_BLOCKED: &str = "fleet event gate blocked";

/// Statement name, for the context the delivery stamp's failure carries.
const CONTEXT_DELIVERED: &str = "admission delivered stamp";

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

/// What [`afd_events::sql::INSERT_LEASED_FLEET_EVENT`] answers: whether this
/// call wrote the row, and the instant the row carries.
#[derive(sqlx::FromRow)]
struct Landed {
    inserted: bool,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
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

impl Leases {
    /// Open the narrative log for `acquired`, if it is not already open.
    ///
    /// Answers [`Delivery::Repeat`] when the row was already there — the
    /// `ON CONFLICT` arm — which is how a re-leased or re-polled event is told
    /// apart from a new one without a second read. A first delivery also
    /// answers the fleet's counters, read after commit on the same
    /// connection.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. The workspace is already a
    /// [`Uuid7`] by the time it arrives — the envelope
    /// parsed it — so there is nothing left here to validate.
    pub async fn record_received(&self, acquired: &Acquired) -> Result<Received> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_RECEIVED))?;
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
        let admission = Uuid7::parse(&acquired.event_id).ok();
        let landed: Landed = sqlx::query_as(afd_events::sql::INSERT_LEASED_FLEET_EVENT)
            .bind(acquired.fleet_id.as_str())
            .bind(&acquired.event_id)
            .bind(acquired.workspace_id.as_str())
            .bind(&acquired.actor)
            .bind(&acquired.event_type)
            .bind(&acquired.request_json)
            .bind(Option::<&str>::None)
            .bind(afd_core::event::status::RECEIVED)
            .bind(admission.as_ref().map(Uuid7::as_str))
            .bind(afd_admission::Producer::Steer.as_str())
            .fetch_one(&mut *transaction)
            .await
            .map_err(query(CONTEXT_RECEIVED))?;
        let Landed {
            inserted,
            created_at: opened_at,
        } = landed;

        sqlx::query(queue::STAMP_DELIVERY)
            .bind(acquired.fleet_id.as_str())
            .bind(&acquired.event_id)
            .bind(acquired.fence.as_i64())
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_DELIVERED))?;

        // A false flag is the conflict arm: the row was already there, so
        // somebody has already paid for this event. `rows_affected` cannot say
        // that any more — the arm converges to keep a continuation's
        // predecessor, so it reports one row written either way.
        let delivery = if inserted {
            Delivery::First
        } else {
            self.redelivery_of(&mut transaction, acquired).await?
        };
        transaction
            .commit()
            .await
            .map_err(query(CONTEXT_RECEIVED))?;
        // Read optional tail figures after commit so a refused SELECT cannot
        // abort the durable delivery.
        let counters = if inserted {
            afd_events::fleet_counters_best_effort_on(&mut connection, acquired.fleet_id.as_str())
                .await
        } else {
            None
        };
        Ok(Received {
            delivery,
            counters,
            opened_at,
        })
    }

    /// Which kind of redelivery this is: one that may still run, or one that
    /// has already finished.
    ///
    /// Open means `received` and nothing else, which is not a shortcut — it is
    /// the same predicate the failure and report updates guard on. Every other
    /// stored spelling is an ending somebody already wrote: a runner's
    /// `processed` or `fleet_error`, or a daemon-side refusal like
    /// `gate_blocked`, which `afd_core::event::status` documents as terminal in
    /// its own right. Asking "is it still open" rather than listing the endings
    /// is what keeps this correct when a new refusal spelling is added.
    ///
    /// A row that vanished between the insert and this read is treated as still
    /// open. That is the conservative direction: the alternative is dropping an
    /// event nobody can prove ran, and a redelivery that runs twice is visible
    /// while one that is silently discarded is not.
    async fn redelivery_of(
        &self,
        connection: &mut sqlx::PgConnection,
        acquired: &Acquired,
    ) -> Result<Delivery> {
        let stored: Option<String> = sqlx::query_scalar(afd_events::sql::SELECT_FLEET_EVENT_STATUS)
            .bind(acquired.fleet_id.as_str())
            .bind(&acquired.event_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_RECEIVED))?;
        Ok(match stored.as_deref() {
            Some(afd_core::event::status::RECEIVED) | None => Delivery::Repeat,
            Some(_) => Delivery::Terminal,
        })
    }
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
            .bind(acquired.fleet_id.as_str())
            .bind(&acquired.event_id)
            .bind(afd_core::event::status::GATE_BLOCKED)
            .bind(refusal.label)
            .bind(afd_core::event::status::RECEIVED)
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
