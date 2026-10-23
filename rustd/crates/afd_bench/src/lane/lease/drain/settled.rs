//! Read durable event, usage and unsettled admission counts after the drain.

use afd_core::event::status::PROCESSED;

use crate::datastores::Datastores;
use crate::error::Result;
use crate::lane::lease::seed::SeededFleet;

/// What the drained population holds once the runners stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Settled {
    /// Event rows the drained fleets carry, whatever their status.
    pub(super) event_rows: u64,
    /// Of those, the ones a report closed as processed.
    pub(super) processed: u64,
    /// Ledger rows charged to the drained fleets.
    pub(super) ledger_rows: u64,
    /// Unsettled admissions belonging to the drained fleets.
    pub(super) unsettled_rows: u64,
}

/// [`SELECT_EVENT_COUNTS`]'s one row: every event row, then the processed ones.
#[derive(Debug, sqlx::FromRow)]
struct EventCounts(i64, i64);

/// A population's event rows, and how many of them settled processed.
const SELECT_EVENT_COUNTS: &str = include_str!("sql/select_event_counts.sql");

/// A population's charge rows: one per settled half of an event.
const SELECT_CHARGE_COUNT: &str = include_str!("sql/select_charge_count.sql");

const SELECT_UNSETTLED_COUNT: &str = include_str!("sql/select_unsettled_count.sql");

/// Read the drained population back.
///
/// # Errors
///
/// Whatever Postgres refused.
pub(super) async fn read(stores: &Datastores, fleets: &[SeededFleet]) -> Result<Settled> {
    let ids: Vec<&str> = fleets.iter().map(|it| it.fleet.as_str()).collect();
    let mut connection = stores.database.acquire().await?;
    let events: EventCounts = sqlx::query_as(SELECT_EVENT_COUNTS)
        .bind(&ids)
        .bind(PROCESSED)
        .fetch_one(&mut *connection)
        .await?;
    let ledger: i64 = sqlx::query_scalar(SELECT_CHARGE_COUNT)
        .bind(&ids)
        .fetch_one(&mut *connection)
        .await?;
    Ok(Settled {
        event_rows: count(events.0),
        processed: count(events.1),
        ledger_rows: count(ledger),
        unsettled_rows: count(
            sqlx::query_scalar(SELECT_UNSETTLED_COUNT)
                .bind(&ids)
                .fetch_one(&mut *connection)
                .await?,
        ),
    })
}

/// A `count(*)`, which Postgres answers as a non-negative bigint.
fn count(value: i64) -> u64 {
    value.unsigned_abs()
}
