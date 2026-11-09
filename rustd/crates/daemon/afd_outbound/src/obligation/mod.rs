//! `fleet.obligations` — every statement that touches the table, and the
//! shapes that address it.
//!
//! # Why one module owns the table
//!
//! The table lives here, in the crate that DELIVERS, and `afd_fleet` calls
//! [`owe`] inside its report transaction. The dependency runs fleet → outbound
//! and nowhere back.
//!
//! # Postgres is the queue (M244 §4)
//!
//! An owed row is free to claim the moment its report commits. A worker claims
//! lane heads — the earliest open row of each (provider, workspace) pair — with
//! its own token in `receipt`, delivers, and stamps, counts, releases or
//! abandons only while that token still holds. A worker that dies mid-delivery
//! simply stops renewing; its claim lapses after [`CLAIM_TTL`] and the next
//! claim pass retakes the row, while the dead holder's late write matches
//! nothing.

use std::time::Duration;

use af_core::id::Uuid7;
use afd_connector::Provider;
use afd_db::Db;

use crate::error::Result;

mod outcome;
pub(crate) mod sql;

pub use self::outcome::{AbandonReason, abandon, count_attempt, stamp_delivered};

/// How long a claim holds without renewal.
pub const CLAIM_TTL: Duration = Duration::from_secs(30);
/// How often a holder renews while it delivers; a third of [`CLAIM_TTL`].
pub const CLAIM_RENEW: Duration = Duration::from_secs(10);
const _: () = assert!(
    CLAIM_RENEW.as_millis() * 3 <= CLAIM_TTL.as_millis(),
    "a holder renews at least three times inside one claim"
);

/// The longest renewal interval a worker accepts: two renewals may fail and
/// the third still lands inside one claim.
pub const CLAIM_RENEW_MAX: Duration = Duration::from_secs(CLAIM_TTL.as_secs() / 3);

/// Statement name, for the context a claim failure carries.
const CONTEXT_CLAIM: &str = "claim obligations";
/// Statement name, for the context a renewal or release failure carries.
const CONTEXT_HOLD: &str = "hold obligation claim";
/// Statement name, for the context an owe failure carries.
const CONTEXT_OWE: &str = "owe delivery";

/// One owed delivery, as a caller ADDRESSES it.
///
/// A struct rather than five positional `&str`-shaped parameters: four of these
/// are text, and a transposition between the event and the answer — or the
/// provider and the workspace — writes a wrong row and compiles clean.
#[derive(Debug, Clone, Copy)]
pub struct Delivery<'a> {
    /// The fleet that produced the answer.
    pub fleet_id: &'a Uuid7,
    /// The workspace whose grant pays for it.
    pub workspace_id: &'a Uuid7,
    /// Which connector carries it back. The connector type rather than its
    /// id, so a lease's model provider cannot be passed here and compile.
    pub provider: Provider,
    /// Where that connector posts it, as the producer recorded it at admission.
    pub destination: &'a str,
    /// The event the answer is threaded onto.
    pub event_id: &'a Uuid7,
    /// What to say.
    pub answer: &'a str,
}

/// What a poster needs to carry one answer: whose it is, where it goes, and
/// what it says. A claimed row carries one, and so does an interjection, which
/// is posted without ever being owed.
///
/// Decoded by `sqlx::FromRow`, which binds by column name, so a reordered
/// `RETURNING` list cannot silently shift a field.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Outgoing {
    /// The fleet that produced the answer.
    pub fleet_id: Uuid7,
    /// The workspace whose grant pays for the delivery.
    pub workspace_id: Uuid7,
    /// Which connector carries it back, as the ledger spells it.
    pub provider: String,
    /// Where that connector posts it.
    pub destination: String,
    /// The event the answer is threaded onto.
    pub event_id: Uuid7,
    /// What to say.
    pub answer: String,
}

/// One claimed answer, as the holder delivers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claimed {
    /// The obligation row.
    pub id: Uuid7,
    /// What the row asks to be posted.
    pub outgoing: Outgoing,
    /// The claim's token; every later write on this row matches it.
    pub token: Uuid7,
}

/// One row the claim statement returns.
#[derive(sqlx::FromRow)]
struct ClaimedRow {
    id: Uuid7,
    #[sqlx(flatten)]
    outgoing: Outgoing,
}

/// Owe this answer its delivery, on the caller's connection.
///
/// Runs inside the report's transaction, so the obligation and the result it
/// describes share one fate. The row is written with the report's own token
/// and an expired claim, so it is free to claim the moment the transaction
/// commits.
///
/// An EMPTY answer owes nothing: a row carrying an empty string would post a
/// blank message into a real thread.
///
/// Answers `true` when this call created the row — `false` for an empty answer
/// or a repeat that conflicted — so a caller pings only for what it wrote.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn owe(
    connection: &mut sqlx::PgConnection,
    row_id: &Uuid7,
    token: &Uuid7,
    delivery: Delivery<'_>,
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
        .bind(delivery.destination)
        .bind(token)
        .fetch_optional(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_OWE))?;
    Ok(written.is_some())
}

/// Claims up to `limit` lane heads for `token`, in `sequence` order.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn claim(database: &Db, limit: i64, token: &Uuid7) -> Result<Vec<Claimed>> {
    let mut connection = database.acquire().await?;
    let rows = sqlx::query_as::<_, ClaimedRow>(sql::CLAIM_HEADS)
        .bind(limit)
        .bind(token)
        .bind(CLAIM_TTL)
        .fetch_all(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_CLAIM))?;
    Ok(rows
        .into_iter()
        .map(|row| Claimed {
            id: row.id,
            outgoing: row.outgoing,
            token: token.clone(),
        })
        .collect())
}

/// Extends `job`'s claim; `false` when it lapsed and another holder may own it.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn renew(database: &Db, job: &Claimed) -> Result<bool> {
    hold(database, sql::RENEW_CLAIM, job, CLAIM_TTL).await
}

/// Gives `job`'s claim back, due again after `after`.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn release(database: &Db, job: &Claimed, after: Duration) -> Result<bool> {
    hold(database, sql::RELEASE_CLAIM, job, after).await
}

async fn hold(
    database: &Db,
    statement: &'static str,
    job: &Claimed,
    span: Duration,
) -> Result<bool> {
    let mut connection = database.acquire().await?;
    let done = sqlx::query(statement)
        .bind(&job.id)
        .bind(&job.token)
        .bind(span)
        .execute(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_HOLD))?;
    Ok(done.rows_affected() == 1)
}
