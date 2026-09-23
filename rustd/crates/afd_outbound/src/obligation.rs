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

use afd_connector::Provider;
use afd_core::clock::UnixMillis;
use afd_db::Db;
use afd_dragonfly::OutboundJob;

use crate::error::Result;

mod sql;

/// Statement name, for the context a failure carries.
const CONTEXT_STAMP: &str = "stamp delivered";

/// Statement name, for the context a cycle-start failure carries.
const CONTEXT_COUNT: &str = "count delivery attempt";

/// Statement name, for the context a scan failure carries.
const CONTEXT_SCAN: &str = "scan obligations";

/// Statement name, for the context an abandon failure carries.
const CONTEXT_ABANDON: &str = "abandon obligation";

/// Why an answer was given up on.
///
/// A closed set whose spelling is what `abandon_reason` stores, so an operator
/// filtering on it and the code writing it cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbandonReason {
    /// The destination refused it and no retry changes that: a deleted
    /// channel, a removed bot, an address naming nowhere. The poster's own
    /// failure event names which.
    Refused,
    /// Every delivery cycle it was allowed ended retryable.
    CyclesExhausted,
    /// Its stored connector id names no connector, so no queue entry could
    /// deliver it: a connector removed from the catalogue, or an edit made
    /// out of band.
    Unaddressable,
}

impl AbandonReason {
    /// The stored spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Refused => "refused",
            Self::CyclesExhausted => "cycles_exhausted",
            Self::Unaddressable => "unaddressable",
        }
    }
}

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
    /// Which connector carries it back. The connector type rather than its
    /// id, so a lease's model provider — a string the report also holds —
    /// cannot be passed here and compile.
    pub provider: Provider,
    /// Where that connector posts it: an address only its poster reads, as the
    /// producer recorded it at admission.
    pub destination: &'a str,
    /// The event the answer is threaded onto.
    pub event_id: &'a str,
    /// What to say.
    pub answer: &'a str,
}

impl<'a> From<Delivery<'a>> for OutboundJob<'a> {
    /// The queue entry that carries a delivery to its poster.
    ///
    /// The one conversion, so the report's append and the producer's re-append
    /// cannot disagree about which field goes where.
    fn from(delivery: Delivery<'a>) -> Self {
        Self {
            provider: delivery.provider.id(),
            destination: delivery.destination,
            workspace_id: delivery.workspace_id,
            fleet_id: delivery.fleet_id,
            event_id: delivery.event_id,
            answer: delivery.answer,
        }
    }
}

impl Owed {
    /// This row as a caller would address it, or `None` when it cannot be.
    ///
    /// The one place the owned and borrowed halves meet, so a scan hands its
    /// rows to the same append the report path uses and neither side carries a
    /// second opinion about which fields identify a delivery.
    ///
    /// `None` for a row with no destination, or whose stored connector id no
    /// connector answers to: both are rows written before an obligation had to
    /// name where it goes, owed to a model provider, and no append can deliver
    /// them.
    #[must_use]
    pub fn addressed(&self) -> Option<Delivery<'_>> {
        Some(Delivery {
            fleet_id: &self.fleet_id,
            workspace_id: &self.workspace_id,
            provider: Provider::parse(&self.provider)?,
            destination: self.destination.as_deref()?,
            event_id: &self.event_id,
            answer: &self.answer,
        })
    }
}

/// One answer this deployment still owes, as the LEDGER holds it.
///
/// The owning half: a scan reads rows out of a transient result set, so it has
/// to own its text. [`Owed::addressed`] is how it becomes a [`Delivery`].
///
/// Decoded by `sqlx::FromRow` rather than seven hand-written `try_get` calls:
/// the derive binds by COLUMN NAME, so it cannot be silently broken by editing
/// a `SELECT` list into a different order the way a positional decoder can.
/// Both scans select the same seven names for exactly that reason.
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
    /// Which connector carries it back, as the ledger spells it.
    pub provider: String,
    /// Where that connector posts it; absent on rows written before an
    /// obligation had to name one.
    pub destination: Option<String>,
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
    let written = sqlx::query(sql::OWE_DELIVERY)
        .bind(row_id)
        .bind(delivery.fleet_id)
        .bind(delivery.workspace_id)
        .bind(delivery.provider.id())
        .bind(delivery.event_id)
        .bind(delivery.answer)
        .bind(now.as_millis())
        .bind(delivery.destination)
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
    write_receipt(database, sql::RECEIPT_DELIVERY, obligation, entry, now).await
}

/// Answers committed but never queued.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn unreceipted(database: &Db, before: UnixMillis, limit: i64) -> Result<Vec<Owed>> {
    scan(database, sql::SELECT_UNRECEIPTED, before, limit).await
}

/// Answers queued but never received.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn undelivered(database: &Db, before: UnixMillis, limit: i64) -> Result<Vec<Owed>> {
    scan(database, sql::SELECT_UNDELIVERED, before, limit).await
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
    write_receipt(database, sql::REAPPEND_RECEIPT, obligation, entry, now).await
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

/// Record that a worker has taken this obligation for a delivery cycle.
///
/// Answers the count this call produced, or `None` when the row was already
/// delivered and nothing was counted — which is what a duplicate queue entry
/// for an answer somebody already received looks like from here.
///
/// Called at the START of the cycle, so the number survives the cycle failing.
/// That makes it best-effort in one direction and only one: a process that dies
/// between this write and the delivery has counted a cycle that produced
/// nothing, and a process that dies before it has delivered a cycle it never
/// counted. Neither can move `delivered_at`, which is the fact anything
/// downstream acts on.
///
/// # Errors
/// Reports a database that would not answer. A caller must log that and DELIVER
/// ANYWAY: the answer is owed to a person and bookkeeping is not.
pub async fn count_attempt(
    database: &Db,
    fleet_id: &str,
    event_id: &str,
    now: UnixMillis,
) -> Result<Option<i64>> {
    let mut connection = database.acquire().await?;
    let counted: Option<(i64,)> = sqlx::query_as(sql::COUNT_ATTEMPT)
        .bind(fleet_id)
        .bind(event_id)
        .bind(now.as_millis())
        .fetch_optional(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_COUNT))?;
    Ok(counted.map(|(count,)| count))
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
    sqlx::query(sql::STAMP_DELIVERED)
        .bind(fleet_id)
        .bind(event_id)
        .bind(now.as_millis())
        .execute(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_STAMP))?;
    Ok(())
}

/// Record that nobody can take this answer, so no scan offers it again.
///
/// Answers the attempt count when THIS call stamped the row, and `None` when
/// the row was already delivered or abandoned — so a caller announces an
/// abandonment once, whatever duplicate entries reach it.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn abandon(
    database: &Db,
    fleet_id: &str,
    event_id: &str,
    reason: AbandonReason,
    now: UnixMillis,
) -> Result<Option<i64>> {
    let mut connection = database.acquire().await?;
    let stamped: Option<(i64,)> = sqlx::query_as(sql::ABANDON)
        .bind(fleet_id)
        .bind(event_id)
        .bind(now.as_millis())
        .bind(reason.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_ABANDON))?;
    Ok(stamped.map(|(attempts,)| attempts))
}
