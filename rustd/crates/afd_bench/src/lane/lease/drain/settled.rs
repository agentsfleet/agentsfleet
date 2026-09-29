//! Where the drained population ended up, read back from both datastores.
//!
//! The drain's numbers mean what their names say only if the work actually
//! settled: every event closed once, each one charged as a received event and
//! as a run, and the readiness index left holding nothing for fleets with no
//! work. So these are read AFTER the drain and written beside its cost, and
//! the lane's test asserts on them rather than on a count the runners kept.

use afd_core::event::status::PROCESSED;
use afd_dragonfly::ReadyIndex;
use sqlx::Row as _;

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
    /// Drained fleets the readiness index still marks.
    ///
    /// Counted per fleet rather than as the whole index's depth: the index is
    /// shared with every other writer on the rig, and their marks say nothing
    /// about whether THIS population drained.
    pub(super) ready_depth: u64,
}

/// Read the drained population back.
///
/// # Errors
///
/// Whatever Postgres or Dragonfly refused.
pub(super) async fn read(stores: &Datastores, fleets: &[SeededFleet]) -> Result<Settled> {
    let ids: Vec<&str> = fleets.iter().map(|it| it.fleet.as_str()).collect();
    let mut connection = stores.database.acquire().await?;
    let events = sqlx::query(
        "SELECT count(*), count(*) FILTER (WHERE status = $2) \
         FROM core.fleet_events WHERE fleet_id = ANY($1::uuid[])",
    )
    .bind(&ids)
    .bind(PROCESSED)
    .fetch_one(&mut *connection)
    .await?;
    let ledger: i64 =
        sqlx::query("SELECT count(*) FROM billing.usage_ledger WHERE fleet_id = ANY($1::uuid[])")
            .bind(&ids)
            .fetch_one(&mut *connection)
            .await?
            .try_get(0)?;
    Ok(Settled {
        event_rows: count(events.try_get(0)?),
        processed: count(events.try_get(1)?),
        ledger_rows: count(ledger),
        ready_depth: marked(stores, fleets).await?,
    })
}

/// How many of `fleets` the readiness index still marks.
///
/// # Errors
///
/// Whatever Dragonfly refused.
pub(super) async fn marked(stores: &Datastores, fleets: &[SeededFleet]) -> Result<u64> {
    let index = ReadyIndex::new(stores.queue.clone());
    let mut marked = 0;
    for fleet in fleets {
        if index.token_for(&fleet.fleet).await?.is_some() {
            marked += 1;
        }
    }
    Ok(marked)
}

/// A `count(*)`, which Postgres answers as a non-negative bigint.
fn count(value: i64) -> u64 {
    value.unsigned_abs()
}
