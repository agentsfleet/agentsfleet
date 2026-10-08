//! The sweep: every gate whose window closed with no answer, expired at once.
//!
//! Split from [`super`] because the sweep is the one verb that moves MANY rows
//! in one statement and then owes the tail one frame per row. The statement
//! hands back each row's fleet, event and the fleet's count after the sweep;
//! the loop below reads the fleet's counters and wakes the fleet once per
//! distinct fleet, then decodes and announces. A backlog of N expired gates
//! across F fleets costs F key lookups and F marks, not N.
//!
//! # Every swept fleet is woken
//!
//! A delivery parked on a gate cleared its fleet's readiness mark, because
//! the answer was going to re-mark it. An expiry IS that answer, and nobody
//! else gives it: without the mark, no poll reads the lapsed gate and the
//! parked delivery never ends.
//!
//! # The rows decode by construction
//!
//! The statement casts both ids to text from NOT NULL columns, reads the
//! nullable event as an option and counts with `COUNT(*)`, so every row it
//! returns decodes. A decode failure could only be this build's statement and
//! decoder disagreeing, and it is reported rather than skipped — after every
//! row that did decode is woken and announced, since the statement already
//! expired them all.

use std::collections::BTreeMap;

use afd_api_wire::approval::status;
use afd_api_wire::tail::FleetCounters;
use afd_core::clock::UnixMillis;
use sqlx::Row as _;

use super::Inbox;
use super::announce::Answer;
use crate::sql;
use crate::{Result, error};

/// Who a swept gate records as its resolver.
const SWEEPER: &str = "system:approval_gate_sweeper";

/// What a swept gate records as its detail.
const SWEPT_DETAIL: &str = "the approval window closed with no answer";

const CONTEXT_EXPIRE: &str = "gate.inbox.expire";

/// What a swept row hands back, and where its announcement goes.
///
/// Read off [`sql::EXPIRE_GATES`]'s select so a sweep of many gates announces
/// each on its own fleet's tail without a read per row — the count the frame
/// carries rode the same statement. The event is optional because the column
/// is: a gate raised on a standing grant rather than on a run parks no event.
struct Swept {
    gate: String,
    fleet: String,
    event: Option<String>,
    pending: i64,
}

/// One swept row as a test hands it to [`Inbox::serve_swept`]:
/// `(gate, fleet, event, pending)`.
#[cfg(feature = "test-util")]
pub type SweptParts = (String, String, Option<String>, i64);

impl Inbox {
    /// Expires every gate whose deadline has passed, reporting how many.
    ///
    /// Scoped to PENDING rows, so an answer that landed a millisecond before
    /// the deadline is not overwritten: the operator's decision outranks the
    /// clock's.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row the decoder does
    /// not match — which only a statement and decoder out of step can cause.
    pub async fn expire(&self, now: UnixMillis) -> Result<u64> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::EXPIRE_GATES)
            .bind(status::TIMED_OUT)
            .bind(status::PENDING)
            .bind(SWEEPER)
            .bind(SWEPT_DETAIL)
            .bind(now.as_millis())
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_EXPIRE))?;
        drop(connection);

        self.serve(rows.iter().map(Self::swept)).await?;
        Ok(rows.len() as u64)
    }

    /// Wakes and announces every row that decoded, then answers the first
    /// that did not.
    ///
    /// The gates are already expired when this runs, so a row that fails to
    /// decode must not strand the ones that did: each of those fleets parked a
    /// delivery that only this wake ends. The first failure is still raised,
    /// because a decode mismatch is this build's statement and decoder out of
    /// step, and every later one is that same cause again.
    async fn serve(&self, rows: impl IntoIterator<Item = Result<Swept>>) -> Result<()> {
        let mut undecodable = None;
        let mut read: BTreeMap<String, Option<FleetCounters>> = BTreeMap::new();
        for row in rows {
            match row {
                Ok(swept) => self.serve_one(&swept, &mut read).await,
                Err(failure) => {
                    undecodable.get_or_insert(failure);
                }
            }
        }
        undecodable.map_or(Ok(()), Err)
    }

    /// Wakes one swept gate's fleet, once per fleet, and announces the gate.
    async fn serve_one(&self, swept: &Swept, read: &mut BTreeMap<String, Option<FleetCounters>>) {
        let counters = if let Some(counters) = read.get(&swept.fleet) {
            *counters
        } else {
            self.wake_parked_delivery(&swept.fleet, &swept.gate).await;
            let counters =
                afd_events::fleet_counters_best_effort(&self.database, &swept.fleet).await;
            read.insert(swept.fleet.clone(), counters);
            counters
        };
        self.announce(Answer {
            fleet_id: &swept.fleet,
            gate_id: &swept.gate,
            event_id: swept.event.as_deref(),
            status: status::TIMED_OUT,
            resolved_by: SWEEPER,
            pending_approvals: swept.pending,
            counters,
        })
        .await;
    }

    /// [`Self::serve`] over rows a test builds, one of them undecodable.
    ///
    /// The statement cannot return an undecodable row by construction, so the
    /// arm that serves the good rows before raising the bad one is reachable
    /// only through here.
    ///
    /// # Errors
    /// The first `Err` in `rows`, after every `Ok` row is served.
    #[cfg(feature = "test-util")]
    pub async fn serve_swept(&self, rows: Vec<Result<SweptParts>>) -> Result<()> {
        let rows = rows.into_iter().map(|row| {
            row.map(|(gate, fleet, event, pending)| Swept {
                gate,
                fleet,
                event,
                pending,
            })
        });
        self.serve(rows).await
    }

    /// One swept row as the tail hears of it.
    fn swept(row: &sqlx::postgres::PgRow) -> Result<Swept> {
        let unreadable = error::query(CONTEXT_EXPIRE);
        Ok(Swept {
            gate: row.try_get(0).map_err(&unreadable)?,
            fleet: row.try_get(1).map_err(&unreadable)?,
            event: row.try_get(2).map_err(&unreadable)?,
            pending: row.try_get(3).map_err(&unreadable)?,
        })
    }
}
