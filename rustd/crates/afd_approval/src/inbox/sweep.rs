//! The sweep: every gate whose window closed with no answer, expired at once.
//!
//! Split from [`super`] because the sweep is the one verb that moves MANY rows
//! in one statement and then owes the tail one frame per row. The statement
//! hands back each row's fleet, event and the fleet's count after the sweep;
//! the loop below reads the fleet's counters once per distinct fleet, then
//! decodes and announces. A backlog of N expired gates across F fleets costs
//! F key lookups, not N.
//!
//! # Nothing past the statement can fail the sweep
//!
//! The rows are expired when the statement returns. A row this build cannot
//! decode costs the tail its frame and is logged; it never turns a committed
//! sweep into a reported failure a caller would retry against rows already
//! moved.

use std::collections::BTreeMap;

use afd_core::clock::UnixMillis;
use afd_wire::approval::status;
use afd_wire::tail::FleetCounters;
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

/// A swept row could not be decoded after the sweep committed.
const EVENT_SWEPT_UNREADABLE: &str = "gate_sweep_row_unreadable";

/// What a swept row hands back, and where its announcement goes.
///
/// Read off [`sql::EXPIRE_GATES`]'s select so a sweep of many gates announces
/// each on its own fleet's tail without a read per row — the count the frame
/// carries rode the same statement. The event is optional because the column
/// is: a gate raised on a standing grant rather than on a run parks no event,
/// and a sweep that refused to decode such a row would report a failure over
/// rows it had already expired.
struct Swept {
    gate: String,
    fleet: String,
    event: Option<String>,
    pending: i64,
}

impl Inbox {
    /// Expires every gate whose deadline has passed, reporting how many.
    ///
    /// Scoped to PENDING rows, so an answer that landed a millisecond before
    /// the deadline is not overwritten: the operator's decision outranks the
    /// clock's.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
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

        // The rows are swept whatever happens past this line, so a row that
        // will not decode costs the tail its frame and is logged — it never
        // turns a committed sweep into a reported failure.
        let mut read: BTreeMap<String, Option<FleetCounters>> = BTreeMap::new();
        for row in &rows {
            let Some(swept) = Self::swept(row) else {
                continue;
            };
            let counters = if let Some(counters) = read.get(&swept.fleet) {
                *counters
            } else {
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
        Ok(rows.len() as u64)
    }

    /// One swept row as the tail hears of it, or nothing for a row this
    /// build cannot read.
    fn swept(row: &sqlx::postgres::PgRow) -> Option<Swept> {
        let unreadable = error::query(CONTEXT_EXPIRE);
        let read = || -> Result<Swept> {
            Ok(Swept {
                gate: row.try_get(0).map_err(&unreadable)?,
                fleet: row.try_get(1).map_err(&unreadable)?,
                event: row.try_get(2).map_err(&unreadable)?,
                pending: row.try_get(3).map_err(&unreadable)?,
            })
        };
        match read() {
            Ok(swept) => Some(swept),
            Err(fault) => {
                let reason = fault.to_string();
                tracing::warn!(
                    event = EVENT_SWEPT_UNREADABLE,
                    reason,
                    "a swept gate could not be decoded; it is expired and unannounced"
                );
                None
            }
        }
    }
}
