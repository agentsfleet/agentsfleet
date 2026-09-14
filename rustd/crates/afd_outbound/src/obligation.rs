//! Stamping an obligation once a destination has actually taken the answer.
//!
//! The queue acknowledgement is NOT this, and the difference is the whole
//! reason this module exists. An ack says the worker is finished with an entry,
//! and [`Lanes::deliver_and_ack`](crate::lanes) acknowledges an EXHAUSTED job
//! too — deliberately, because leaving it pending would park one undeliverable
//! answer at the head of a destination's lane forever. So "acknowledged" and
//! "delivered" are different facts, and only one of them is what a person on
//! the other end experienced.
//!
//! `delivered_at` is the second. A row that is receipted and still unstamped is
//! an answer the queue carried and nobody received, which is exactly the set
//! the producer's recovery scan re-offers.

use afd_core::clock::UnixMillis;
use afd_db::Db;

use crate::error::Result;

/// Stamp an obligation as delivered, keyed by the answer's own identity.
///
/// Keys on `(fleet_id, event_id)` rather than the obligation's row id because
/// that pair is the table's unique constraint and is already on the job the
/// poster carried — threading a row id through the queue would put a second
/// identity on the wire that has to agree with the first.
///
/// Guarded on `delivered_at IS NULL` so a redelivery cannot move a timestamp
/// that already recorded when the destination first took the answer. The path
/// is at-least-once, so this statement has to be idempotent in the same way the
/// obligation's insert is.
const STAMP_DELIVERED: &str = "\
UPDATE core.fleet_obligations
   SET delivered_at  = $3::bigint,
       attempt_count = attempt_count + 1,
       updated_at    = $3::bigint
 WHERE fleet_id = $1::uuid AND event_id = $2::text AND delivered_at IS NULL";

/// Statement name, for the context a failure carries.
const CONTEXT_STAMP: &str = "stamp delivered";

/// Obligations the queue never confirmed, oldest first.
///
/// The recovery set for a crash between the report's commit and the append that
/// follows it. `created_at < $1` leaves a row alone for its own committer for
/// [`MIN_AGE`](crate::producer::MIN_AGE), so this pass does not race a report
/// that is about to record its own receipt and put a second entry on the queue
/// for an answer already in flight.
///
/// Rides `idx_fleet_obligations_unreceipted`, whose predicate is the same NULL
/// test.
const SELECT_UNRECEIPTED: &str = "\
SELECT id, fleet_id, workspace_id, provider, event_id, answer
  FROM core.fleet_obligations
 WHERE receipt IS NULL AND created_at < $1::bigint
 ORDER BY created_at, seq
 LIMIT $2::bigint";

/// Obligations the queue carried and nobody received, oldest first per fleet.
///
/// This is the set a lost consumer group, a wholly lost stream, and a worker
/// replaced under a different hostname all leave behind — three failures the
/// queue cannot tell apart and none of which it can recover from, because in
/// every one of them the entry is simply gone while the answer is still owed.
///
/// Re-appending is safe precisely because it is not free: the destination may
/// receive the answer twice. That is the trade the whole path makes — at-least
/// once, in a thread a person reads — and it is why `delivered_at` is stamped
/// by the poster rather than by the acknowledgement, so a row only leaves this
/// set when somebody actually got it.
///
/// Rides `idx_fleet_obligations_undelivered`, leading on `fleet_id` because
/// order is promised per destination.
const SELECT_UNDELIVERED: &str = "\
SELECT id, fleet_id, workspace_id, provider, event_id, answer
  FROM core.fleet_obligations
 WHERE receipt IS NOT NULL AND delivered_at IS NULL AND updated_at < $1::bigint
 ORDER BY fleet_id, created_at, seq
 LIMIT $2::bigint";

/// Statement name, for the context a scan failure carries.
const CONTEXT_SCAN: &str = "scan obligations";

/// One answer this deployment still owes.
///
/// Not `Clone`: a scan hands each row to the append once and by value, so a
/// clone here would only ever copy the answer text for nobody.
#[derive(Debug)]
pub struct Owed {
    /// The obligation row.
    pub id: String,
    /// The fleet that produced the answer.
    pub fleet_id: String,
    /// The workspace whose grant pays for the delivery.
    pub workspace_id: String,
    /// Which connector carries it back.
    pub provider: String,
    /// The event the answer is threaded onto.
    pub event_id: String,
    /// What to say.
    pub answer: String,
}

impl Owed {
    /// Read one row of either scan.
    fn read(row: &sqlx::postgres::PgRow) -> Result<Self> {
        use sqlx::Row as _;
        Ok(Self {
            id: row.try_get(0).map_err(crate::error::query(CONTEXT_SCAN))?,
            fleet_id: row.try_get(1).map_err(crate::error::query(CONTEXT_SCAN))?,
            workspace_id: row.try_get(2).map_err(crate::error::query(CONTEXT_SCAN))?,
            provider: row.try_get(3).map_err(crate::error::query(CONTEXT_SCAN))?,
            event_id: row.try_get(4).map_err(crate::error::query(CONTEXT_SCAN))?,
            answer: row.try_get(5).map_err(crate::error::query(CONTEXT_SCAN))?,
        })
    }
}

/// Answers committed but never queued.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn unreceipted(database: &Db, before: UnixMillis, limit: i64) -> Result<Vec<Owed>> {
    scan(database, SELECT_UNRECEIPTED, before, limit).await
}

/// Answers queued but never received.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn undelivered(database: &Db, before: UnixMillis, limit: i64) -> Result<Vec<Owed>> {
    scan(database, SELECT_UNDELIVERED, before, limit).await
}

async fn scan(
    database: &Db,
    statement: &'static str,
    before: UnixMillis,
    limit: i64,
) -> Result<Vec<Owed>> {
    let mut connection = database.acquire().await?;
    let rows = sqlx::query(statement)
        .bind(before.as_millis())
        .bind(limit)
        .fetch_all(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_SCAN))?;
    rows.iter().map(Owed::read).collect()
}

/// Record the queue entry an obligation was re-appended to.
///
/// Unguarded on the receipt, unlike the report path's own: this pass only ever
/// reaches rows the queue lost, and refusing to move a receipt that points at a
/// vanished entry would strand the row forever.
const REAPPEND_RECEIPT: &str = "\
UPDATE core.fleet_obligations
   SET receipt = $2::text, updated_at = $3::bigint
 WHERE id = $1::uuid";

/// Statement name, for the context a re-append failure carries.
const CONTEXT_REAPPEND: &str = "re-append receipt";

/// Point an obligation at the entry this pass appended for it.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn record_reappended(
    database: &Db,
    obligation: &str,
    receipt: &str,
    now: UnixMillis,
) -> Result<()> {
    let mut connection = database.acquire().await?;
    sqlx::query(REAPPEND_RECEIPT)
        .bind(obligation)
        .bind(receipt)
        .bind(now.as_millis())
        .execute(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_REAPPEND))?;
    Ok(())
}

/// Record that a destination accepted this answer.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn stamp_delivered(
    database: &Db,
    fleet_id: &str,
    event_id: &str,
    now: UnixMillis,
) -> Result<()> {
    let mut connection = database.acquire().await?;
    sqlx::query(STAMP_DELIVERED)
        .bind(fleet_id)
        .bind(event_id)
        .bind(now.as_millis())
        .execute(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_STAMP))?;
    Ok(())
}
