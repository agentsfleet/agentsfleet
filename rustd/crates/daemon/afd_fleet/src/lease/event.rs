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

use af_core::id::Uuid7;
use jiff::Timestamp;
use sqlx::{Connection as _, PgConnection};

use afd_api_wire::tail::FleetCounters;
use afd_events::Closed;
use afd_outbound::obligation::Delivery as Owed;

use crate::error::{Result, query};
use crate::lease::admit::Refusal;
use crate::lease::envelope::Acquired;
use crate::lease::store::Leases;
use crate::lease::unanswered;

/// Statement name, for the context a query failure carries while an event is
/// ended with no report: at a gate, or after its leases kept lapsing.
const CONTEXT_ENDED: &str = "fleet event ended without a report";

/// The stored `failure_detail` of an event the daemon stopped re-leasing.
pub const DETAIL_LEASES_LAPSED: &str = "every lease this event was issued lapsed without a report";

/// How an event ends when no report will end it.
#[derive(Debug, Clone, Copy)]
struct Unreported<'a> {
    /// The terminal status the event row takes.
    status: &'static str,
    /// The stored `failure_reason`.
    label: &'static str,
    /// The stored `failure_detail`, when the ending has one to give.
    detail: Option<&'static str>,
    /// What the event's thread is owed, when it was asked from one.
    notice: &'a str,
}

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
    /// The pass that ends the event also owes its thread the refusal, when it
    /// was asked from one: no run follows a gate, so nothing else would answer.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a closing this daemon
    /// cannot read. A row that has already ended is [`Ended::Already`], not
    /// an error — see that type.
    pub async fn block(&self, acquired: &Acquired, refusal: Refusal) -> Result<Ended> {
        let notice = unanswered::refused(&acquired.fleet_id, refusal.label);
        let ending = Unreported {
            status: af_core::event::status::GATE_BLOCKED,
            label: refusal.label,
            // A gate refusal carries no operator-readable instruction.
            detail: None,
            notice: &notice,
        };
        self.end_unreported(acquired, ending).await
    }

    /// End an event whose leases kept lapsing, as the failed run it is.
    ///
    /// A lapsed lease is reclaimed and its event runs again from the start.
    /// Past [`af_core::limits::EVENT_LEASES_MAX`] holders the daemon stops:
    /// the event ends under the label a lapsed lease reports, its thread is
    /// told, and the fleet's next event is free to run. The caller keeps a
    /// durable task's event out of here: ending it would leave its task
    /// running with nothing left to run it.
    ///
    /// # Errors
    /// As [`Self::block`].
    pub async fn abandon(&self, acquired: &Acquired) -> Result<Ended> {
        let label = af_core::event::label::LEASE_EXPIRED;
        let notice = unanswered::failed_run(&acquired.fleet_id, Some(label));
        let ending = Unreported {
            status: af_core::event::status::FLEET_ERROR,
            label,
            detail: Some(DETAIL_LEASES_LAPSED),
            notice: &notice,
        };
        self.end_unreported(acquired, ending).await
    }

    /// Writes the terminal row, settles the admission and owes the notice, in
    /// one transaction under the claim's fence.
    async fn end_unreported(&self, acquired: &Acquired, ending: Unreported<'_>) -> Result<Ended> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_ENDED))?;
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
            .bind(ending.status)
            .bind(ending.label)
            .bind(af_core::event::status::RECEIVED)
            .bind(ending.detail)
            .bind(afd_api_wire::approval::status::PENDING)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_ENDED))?;

        self.settle_terminal_on(&mut transaction, acquired).await?;
        let (ended, owed) = match closed {
            Some(row) => (
                Ended::Now(Box::new(Closed::read(&row)?)),
                self.owe_notice(&mut transaction, acquired, ending.notice)
                    .await?,
            ),
            None => (Ended::Already, None),
        };
        transaction.commit().await.map_err(query(CONTEXT_ENDED))?;
        if let Some(owed) = owed {
            self.wake_delivery(&owed).await;
        }
        Ok(ended)
    }

    /// Owes the ended event's thread its notice, and answers the row owed.
    ///
    /// `None` for an event asked from nowhere an answer can go: a webhook, a
    /// schedule and a message sent from the dashboard have no thread.
    async fn owe_notice(
        &self,
        connection: &mut PgConnection,
        acquired: &Acquired,
        notice: &str,
    ) -> Result<Option<Uuid7>> {
        let event = acquired.event_id.as_str();
        let Some(reply) = Self::reply_destination(connection, &acquired.fleet_id, event).await?
        else {
            return Ok(None);
        };
        self.owe_delivery(
            connection,
            Owed {
                fleet_id: &acquired.fleet_id,
                workspace_id: &acquired.workspace_id,
                provider: reply.provider,
                destination: &reply.address,
                event_id: &acquired.event_id,
                answer: notice,
            },
            af_core::clock::now(),
        )
        .await
    }
}
