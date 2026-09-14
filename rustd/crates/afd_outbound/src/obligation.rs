//! `core.fleet_obligations` — every statement that touches the table, and the
//! two shapes that address it.
//!
//! # Why one module owns the table
//!
//! The statements were split across this crate and `afd_fleet` — the report
//! path held the insert and its receipt, the delivery path the scans and the
//! stamp — and that split immediately produced what a split always produces:
//! two `UPDATE … SET receipt` statements differing by one guard, and two
//! structs for the same six columns. `afd_events::sql` carries the same lesson
//! in its own note, where `afd_approval` once held a byte-identical copy of
//! `INSERT_FLEET_EVENT`.
//!
//! So the table lives here, in the crate that DELIVERS, and `afd_fleet` calls
//! [`owe`] inside its report transaction. The dependency runs fleet → outbound
//! and nowhere back.
//!
//! # Acknowledged is not delivered
//!
//! An ack says the worker is finished with an entry,
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

/// One owed delivery, as a caller ADDRESSES it.
///
/// The borrowing half of the pair below. A struct rather than five positional
/// `&str`-shaped parameters: four of these are text, and a transposition
/// between the event and the answer — or the provider and the workspace —
/// writes a wrong row and compiles clean.
#[derive(Debug, Clone, Copy)]
pub struct Delivery<'a> {
    /// The fleet that produced the answer.
    pub fleet_id: &'a str,
    /// The workspace whose grant pays for it.
    pub workspace_id: &'a str,
    /// Which connector carries it back.
    pub provider: &'a str,
    /// The event the answer is threaded onto.
    pub event_id: &'a str,
    /// What to say.
    pub answer: &'a str,
}

impl Owed {
    /// This row as a caller would address it.
    ///
    /// The one place the owned and borrowed halves meet, so a scan hands its
    /// rows to the same append the report path uses and neither side carries a
    /// second opinion about which five fields identify a delivery.
    #[must_use]
    pub fn addressed(&self) -> Delivery<'_> {
        Delivery {
            fleet_id: &self.fleet_id,
            workspace_id: &self.workspace_id,
            provider: &self.provider,
            event_id: &self.event_id,
            answer: &self.answer,
        }
    }
}

/// One answer this deployment still owes, as the LEDGER holds it.
///
/// The owning half: a scan reads rows out of a transient result set, so it has
/// to own its text. [`Owed::addressed`] is how it becomes a [`Delivery`].
///
/// Decoded by `sqlx::FromRow` rather than six hand-written `try_get` calls:
/// the derive binds by COLUMN NAME, so it cannot be silently broken by editing
/// a `SELECT` list into a different order the way a positional decoder can.
/// Both scans select the same six names for exactly that reason.
///
/// Not `Clone`: a scan hands each row to the append once and by value, so a
/// clone here would only ever copy the answer text for nobody.
#[derive(Debug, sqlx::FromRow)]
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

/// Statement name, for the context an owe failure carries.
const CONTEXT_OWE: &str = "owe delivery";

/// Owe this answer its delivery, on the caller's connection.
///
/// Runs inside the report's transaction, so the obligation and the result it
/// describes share one fate: a rollback that un-does the settle un-does this
/// too, which is the property that makes "committed and charged" and "a
/// delivery is owed" the same instant.
///
/// Takes the row id rather than minting one — the entropy source belongs to the
/// caller's store, and this crate has no business holding one.
///
/// An EMPTY answer owes nothing. A run that produced no output has nothing to
/// say, and a row carrying an empty string would be an obligation the producer
/// enqueues, a poster delivers, and a reader sees as a blank message in a real
/// thread. Deliberate, so it is stated here and asserted in the suite.
///
/// Answers `true` when this call is the one that created the row — `false` for
/// an empty answer or a repeat that conflicted — so a caller appends only what
/// it actually wrote, and a replayed report cannot put a second entry on the
/// queue for an answer already in flight.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn owe(
    connection: &mut sqlx::PgConnection,
    row_id: &str,
    delivery: Delivery<'_>,
    now: UnixMillis,
) -> Result<bool> {
    if delivery.answer.is_empty() {
        return Ok(false);
    }
    let written = sqlx::query(OWE_DELIVERY)
        .bind(row_id)
        .bind(delivery.fleet_id)
        .bind(delivery.workspace_id)
        .bind(delivery.provider)
        .bind(delivery.event_id)
        .bind(delivery.answer)
        .bind(now.as_millis())
        .fetch_optional(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_OWE))?;
    Ok(written.is_some())
}

/// Record the entry an obligation was first appended to.
///
/// Runs on a pooled connection AFTER the report's transaction, because the
/// append it records happens after that transaction too. A failure here leaves
/// the row unreceipted, which is the recoverable state by construction: the
/// producer's scan is exactly that set.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn receipt(database: &Db, obligation: &str, entry: &str, now: UnixMillis) -> Result<()> {
    write_receipt(database, RECEIPT_DELIVERY, obligation, entry, now).await
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
    sqlx::query_as::<_, Owed>(statement)
        .bind(before.as_millis())
        .bind(limit)
        .fetch_all(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_SCAN))
}

/// Owe a delivery, in the report's own transaction.
///
/// The write that closes 7.6's window. `receipt` and `delivered_at` start NULL
/// because the queue append is NOT part of that transaction and cannot be —
/// nothing spans PostgreSQL and Dragonfly — so the append happens after and
/// records its entry id back. Until it does, this row IS the obligation.
///
/// `ON CONFLICT DO NOTHING` on the event, so a re-sent report owes one delivery
/// and not two. That agrees with the settle, which answers a repeat
/// `AlreadySettled` and charges nothing: both halves of a replayed report are
/// no-ops, which is what makes the endpoint idempotent rather than merely
/// idempotent about money. `RETURNING` therefore yields a row only when this
/// call is the one that created it, and the caller appends only what it wrote.
///
/// `$1` row id, `$2` fleet, `$3` workspace, `$4` provider, `$5` event,
/// `$6` answer, `$7` now.
const OWE_DELIVERY: &str = "\
INSERT INTO core.fleet_obligations
  (id, fleet_id, workspace_id, provider, event_id, answer,
   receipt, delivered_at, attempt_count, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::text, $5::text, $6::text,
        NULL, NULL, 0, $7::bigint, $7::bigint)
ON CONFLICT ON CONSTRAINT uq_fleet_obligations_event DO NOTHING
RETURNING id";

/// Record the entry an obligation was appended to, the FIRST time.
///
/// Guarded on the receipt still being NULL so the report path cannot overwrite
/// one the producer recorded first. The loser writes nothing and its entry
/// becomes a duplicate the worker acknowledges without delivering — the safe
/// direction, since the alternative is a row pointing at an entry nobody holds.
///
/// `$1` obligation row, `$2` receipt, `$3` now.
const RECEIPT_DELIVERY: &str = "\
UPDATE core.fleet_obligations
   SET receipt = $2::text, updated_at = $3::bigint
 WHERE id = $1::uuid AND receipt IS NULL";

/// Record the entry an obligation was RE-appended to.
///
/// The sibling above, deliberately without its guard, and the pair is written
/// together so the one difference between them is the thing a reader sees. This
/// pass only ever reaches rows whose entry the queue lost, and the undelivered
/// scan finds rows that already HAVE a receipt — a guarded statement would write
/// nothing there and strand every row it touched.
///
/// `$1` obligation row, `$2` receipt, `$3` now.
const REAPPEND_RECEIPT: &str = "\
UPDATE core.fleet_obligations
   SET receipt = $2::text, updated_at = $3::bigint
 WHERE id = $1::uuid";

/// Statement name, for the context a receipt failure carries.
const CONTEXT_RECEIPT: &str = "record receipt";

/// Point an obligation at the entry the producer re-appended for it.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn record_reappended(
    database: &Db,
    obligation: &str,
    entry: &str,
    now: UnixMillis,
) -> Result<()> {
    write_receipt(database, REAPPEND_RECEIPT, obligation, entry, now).await
}

/// The body both receipt writers share.
///
/// The two statements differ by one guard and nothing else, so they bind the
/// same three parameters in the same order — which is exactly the kind of pair
/// that drifts when each carries its own copy of the binding.
async fn write_receipt(
    database: &Db,
    statement: &'static str,
    obligation: &str,
    entry: &str,
    now: UnixMillis,
) -> Result<()> {
    let mut connection = database.acquire().await?;
    sqlx::query(statement)
        .bind(obligation)
        .bind(entry)
        .bind(now.as_millis())
        .execute(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_RECEIPT))?;
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
