//! Counting what Postgres executed and committed, asked of Postgres itself.
//!
//! # Why the server counts, not a wrapper
//!
//! A pool acquire is not a statement: a report takes one connection and runs
//! several statements on it, so an acquire count cannot see a rewrite that
//! folds three statements into one. Wrapping `sqlx` would count what the
//! wrapper saw, not what the path executed — the reason `instrument.rs` gives
//! for reading the daemon's own counters. So both numbers come from the server:
//! `pg_stat_statements`' `calls` for statements, which it updates as each one
//! finishes, and `pg_stat_database`'s `xact_commit` for commits.
//!
//! # Transaction control is not a statement
//!
//! `BEGIN`, `COMMIT` and their relatives are tracked as utility statements, and
//! counting them would make an explicit transaction look three statements
//! dearer than the same work in autocommit. They are left out of the statement
//! sum because the commit count already carries them. Every other utility
//! statement — the lease path's `SET LOCAL ROLE` among them — does work on a
//! round trip of its own and stays in.
//!
//! # A new connection is not a commit the lane made
//!
//! Every backend commits one transaction of its own while it starts up, to
//! read the catalogues it authenticates against, and `xact_commit` counts it.
//! A pool that opens a connection mid-window — or the rig's own health check,
//! which connects every few seconds — would otherwise add commits no statement
//! explains. `pg_stat_database` counts sessions established beside it, one
//! startup transaction each, so the commit tally is `xact_commit - sessions`.
//!
//! # Commits are flushed before they are read
//!
//! A backend publishes its commit tally lazily: at most once a second, and an
//! idle one may hold its last commits for up to ten seconds. A reading taken
//! the moment a window closes would therefore miss commits the window made. So
//! a reading first asks every connection the lane's pool holds to flush
//! (`pg_stat_force_next_flush`), which each does as it goes idle, waits for
//! each to answer a ping so the flush is known to have landed, and only then
//! reads the two tallies.
//!
//! # The counter's own statements are never prepared
//!
//! A prepared statement's first use on a connection is a Parse round trip,
//! and outside a transaction block Postgres commits that round trip as a
//! transaction of its own — one `pg_stat_statements` never sees, because
//! nothing executed. The pool opens connections as it needs them, so a
//! prepared flush would add an unexplained commit per new connection. Both of
//! the counter's statements go over the simple protocol (`raw_sql`), which has
//! no Parse. The path under measurement keeps its own prepares: those are
//! transactions the server really committed for it.
//!
//! # A delta subtracts its own reading
//!
//! The flushes are statements and commits too, and so is the read. What one
//! reading adds to the next is exact — its read, and the next reading's flushes
//! — so [`StatementReading::since`] subtracts it, and a window in which the
//! lane did nothing reads as zero rather than as the cost of looking.

use afd_db::Db;
use sqlx::pool::PoolConnection;
use sqlx::{Connection as _, Postgres, Row as _};

use crate::error::{Error, LaneFault, Result};

/// Creates the view the counter reads, where the library is already loaded.
const INSTALL: &str = "CREATE EXTENSION IF NOT EXISTS pg_stat_statements";

/// Makes this backend publish its pending tallies as it goes idle.
///
/// It reads a catalogue row on purpose. A backend skips its flush outright
/// when no per-relation statistic is pending, and its commit tally is not one
/// — it is folded in only by a flush that runs for some other reason. So a
/// connection whose work touched no table (`SELECT 1`, or this flush on its
/// own) would hold its commits indefinitely. Reading `pg_database` leaves a
/// relation statistic pending, which is what makes the flush happen.
const FLUSH: &str =
    "SELECT pg_stat_force_next_flush() FROM pg_database WHERE datname = current_database()";

/// Both tallies for the lane's database, in one statement.
///
/// The regex keeps transaction control out of the statement sum (see the
/// module note); `\y` is Postgres's word boundary, so `ENDPOINT` would stay in.
const READ: &str = "SELECT \
       (SELECT COALESCE(sum(calls), 0)::bigint FROM pg_stat_statements \
         WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
           AND query !~* '^\\s*(BEGIN|START TRANSACTION|COMMIT|END|ROLLBACK|SAVEPOINT|RELEASE|ABORT)\\y'), \
       (SELECT xact_commit - sessions FROM pg_stat_database WHERE datname = current_database())";

/// The datastore named when a tally will not read as a count.
const POSTGRES: &str = "postgres";

/// The statement tally, as a refusal names it.
const STATEMENTS_FIELD: &str = "pg_stat_statements.calls";

/// The commit tally, as a refusal names it.
const COMMITS_FIELD: &str = "pg_stat_database.xact_commit - sessions";

/// What one reading costs the next: the read statement, and its commit.
const READ_COST: u64 = 1;

/// The two tallies at one instant, and what taking them cost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatementReading {
    /// Statements executed in this database, transaction control excluded.
    pub statements: u64,
    /// Transactions committed in this database.
    pub commits: u64,
    /// Connections flushed before the read, each one a statement and a commit.
    pub flushed: u64,
}

/// What a window cost Postgres, with the readings' own cost taken out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatementCost {
    /// Statements executed.
    pub statements: u64,
    /// Transactions committed.
    pub commits: u64,
}

impl StatementReading {
    /// What happened between an earlier reading and this one.
    ///
    /// The earlier reading's read, and this reading's flushes, landed inside
    /// the window, so both come off. The earlier reading's flushes and this
    /// reading's read did not.
    #[must_use]
    pub const fn since(self, earlier: Self) -> StatementCost {
        let overhead = READ_COST + self.flushed;
        StatementCost {
            statements: self
                .statements
                .saturating_sub(earlier.statements)
                .saturating_sub(overhead),
            commits: self
                .commits
                .saturating_sub(earlier.commits)
                .saturating_sub(overhead),
        }
    }
}

/// Make the counter readable on this database.
///
/// Idempotent. The library itself must already be preloaded by the server;
/// this only creates the view over it.
///
/// # Errors
///
/// [`LaneFault::StatementsUnreadable`] when Postgres refuses the extension.
pub async fn install(database: &Db) -> Result<()> {
    let mut connection = database.acquire().await?;
    sqlx::query(INSTALL)
        .execute(&mut *connection)
        .await
        .map_err(unreadable)?;
    Ok(())
}

/// Flush every connection the pool holds, then read both tallies.
///
/// # Errors
///
/// [`Error::DatabaseUnavailable`] when a connection will not open, and
/// [`LaneFault::StatementsUnreadable`] when the counter will not answer.
pub async fn read(database: &Db) -> Result<StatementReading> {
    let mut held = every_connection(database).await?;
    for connection in &mut held {
        flush(connection).await?;
    }
    let flushed = u64::try_from(held.len()).unwrap_or(u64::MAX);
    // Read on a connection already in hand and already flushed. Letting the
    // flushed ones go and acquiring again would open a fresh connection while
    // they are still on their way back to the pool, so every reading grew the
    // pool by one and the reading after it had one more connection to flush.
    let row = match held.first_mut() {
        Some(connection) => sqlx::raw_sql(READ).fetch_one(&mut **connection).await,
        None => {
            sqlx::raw_sql(READ)
                .fetch_one(&mut *database.acquire().await?)
                .await
        }
    }
    .map_err(unreadable)?;
    Ok(StatementReading {
        statements: tally(row.try_get(0).map_err(unreadable)?, STATEMENTS_FIELD)?,
        commits: tally(row.try_get(1).map_err(unreadable)?, COMMITS_FIELD)?,
        flushed,
    })
}

/// A server tally as a count, refusing the negative no tally can hold.
fn tally(value: i64, field: &'static str) -> Result<u64> {
    u64::try_from(value).map_err(|_negative| Error::CounterUnreadable {
        datastore: POSTGRES,
        field,
    })
}

/// Every connection the pool holds, all in hand at once.
///
/// Held together, so each acquire hands back a DIFFERENT connection: one at a
/// time would flush the same idle connection over and over and leave the rest
/// holding their commits.
///
/// Acquired until every connection the pool counts is in hand, not a fixed
/// `size()` times. A connection dropped a moment ago is still on its way back
/// to the pool, so an acquire can open a new one instead, and a loop that
/// stopped at the count it started with would leave the returning one — and
/// the commits it holds — unflushed. That was measured: a reading taken right
/// after the extension was installed missed three commits exactly this way.
async fn every_connection(database: &Db) -> Result<Vec<PoolConnection<Postgres>>> {
    let mut held = Vec::new();
    while u64::try_from(held.len()).unwrap_or(u64::MAX) < u64::from(database.size()) {
        held.push(database.acquire().await?);
    }
    Ok(held)
}

/// Make one connection publish what it holds, and wait until it has.
async fn flush(connection: &mut PoolConnection<Postgres>) -> Result<()> {
    sqlx::raw_sql(FLUSH)
        .execute(&mut **connection)
        .await
        .map_err(unreadable)?;
    // `execute` answers at the statement's completion, before the server has
    // handled the Sync that ends its transaction — and the flush runs there. A
    // ping is a bare Sync round trip and opens no transaction, so once it
    // answers the flush before it has landed. Without it the read raced the
    // flushes and a window's commits drifted into the next one.
    connection.ping().await.map_err(unreadable)
}

/// The counter's own refusal, carrying what Postgres said.
fn unreadable(source: sqlx::Error) -> Error {
    LaneFault::StatementsUnreadable { source }.into()
}

#[cfg(test)]
mod tests;
