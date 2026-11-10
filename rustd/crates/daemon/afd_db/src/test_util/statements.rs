//! How many statements a piece of work sent, read from `pg_stat_statements`.
//!
//! The extension counts per database, so a count on the shared lane would
//! include every parallel suite's calls. [`counted_database`] gives a test a
//! migrated database of its own with the extension installed.
//!
//! A [`Window`] reads every query's calls when it opens and again when it
//! closes; [`Ran`] is the difference. Seeds run before the window opens and
//! assertions after it closes, so neither lands in the count. Only top-level
//! statements count, because a trigger's statements are not round trips.
//! Transaction control and the counter's own reads are left out, as the bench
//! leaves them out (`afd_bench`'s statement queries).
//!
//! No flush is needed: the extension counts a call as its statement ends.

use std::collections::BTreeMap;

use crate::config::DbRole;
use crate::migrate::Migrator;
use crate::pool::Db;
use crate::test_util::TestDatabase;

/// Every counted query's calls in this database, one row per query id.
const READ: &str = "SELECT queryid, sum(calls)::bigint, min(query) FROM pg_stat_statements \
       WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
         AND toplevel \
         AND query !~* '^\\s*(BEGIN|START TRANSACTION|COMMIT|END|ROLLBACK|SAVEPOINT|RELEASE|ABORT)\\y' \
         AND query !~ '(pg_stat_|pg_database|pg_extension)' \
       GROUP BY queryid";

/// Installs the counter's views in the current database.
const INSTALL: &str = "CREATE EXTENSION IF NOT EXISTS pg_stat_statements";

/// A migrated database of the caller's own with the counter installed, and
/// its migrator pool, which the window reads through.
///
/// # Panics
/// When the database will not migrate or the extension will not install: the
/// lane's Postgres was started without `pg_stat_statements` preloaded.
pub async fn counted_database() -> (TestDatabase, Db) {
    let database = TestDatabase::create().await;
    let owner = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .run(&owner)
        .await
        .unwrap_or_else(|failure| panic!("the counted database must migrate: {failure}"));
    let mut connection = owner.acquire().await.unwrap_or_else(|failure| {
        panic!("the counted database must accept a connection: {failure}")
    });
    sqlx::raw_sql(INSTALL)
        .execute(&mut *connection)
        .await
        .unwrap_or_else(|failure| {
            panic!("pg_stat_statements must install; is it preloaded? {failure}")
        });
    drop(connection);
    (database, owner)
}

/// Calls and normalized text, by query id, at one instant.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Reading(BTreeMap<i64, (u64, String)>);

impl Reading {
    async fn read(owner: &Db) -> Self {
        let mut connection = owner
            .acquire()
            .await
            .unwrap_or_else(|failure| panic!("the counter must reach its database: {failure}"));
        // The simple protocol, so the read prepares nothing of its own.
        let rows: Vec<(i64, i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(READ))
            .persistent(false)
            .fetch_all(&mut *connection)
            .await
            .unwrap_or_else(|failure| panic!("pg_stat_statements must answer: {failure}"));
        Self(
            rows.into_iter()
                .map(|(id, calls, query)| (id, (u64::try_from(calls).unwrap_or(0), query)))
                .collect(),
        )
    }

    /// Every query that ran between `earlier` and this reading.
    fn since(&self, earlier: &Self) -> Ran {
        Ran(self
            .0
            .iter()
            .filter_map(|(id, (calls, query))| {
                let before = earlier.0.get(id).map_or(0, |(calls, _)| *calls);
                let ran = calls.saturating_sub(before);
                (ran > 0).then(|| (query.clone(), ran))
            })
            .collect())
    }
}

/// An open count over one database.
#[derive(Debug)]
pub struct Window {
    owner: Db,
    opened: Reading,
}

impl Window {
    /// Starts counting. Statements that ended before this call are not counted.
    pub async fn open(owner: &Db) -> Self {
        Self {
            owner: owner.clone(),
            opened: Reading::read(owner).await,
        }
    }

    /// Stops counting and answers what ran since [`Window::open`].
    pub async fn close(self) -> Ran {
        Reading::read(&self.owner).await.since(&self.opened)
    }
}

/// The statements one window counted: each query's normalized text and calls.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ran(Vec<(String, u64)>);

impl Ran {
    /// Every counted statement's calls, summed.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.0.iter().map(|(_, calls)| calls).sum()
    }

    /// Calls of every statement whose text holds `fragment`.
    ///
    /// A fragment, not the whole text: the extension replaces literals with
    /// placeholders, so a source constant holding one never equals its row.
    #[must_use]
    pub fn calls_containing(&self, fragment: &str) -> u64 {
        self.0
            .iter()
            .filter(|(query, _)| query.contains(fragment))
            .map(|(_, calls)| calls)
            .sum()
    }

    /// Each counted statement and its calls, for an assertion's message.
    #[must_use]
    pub fn statements(&self) -> &[(String, u64)] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{Ran, Reading};

    fn reading(rows: &[(i64, u64, &str)]) -> Reading {
        Reading(
            rows.iter()
                .map(|(id, calls, query)| (*id, (*calls, (*query).to_owned())))
                .collect(),
        )
    }

    /// A query seen before the window counts only its new calls, and one first
    /// seen at the close counts every call.
    #[test]
    fn window_counts_only_calls_after_it_opened() {
        let opened = reading(&[(1, 4, "SELECT a"), (2, 9, "UPDATE b")]);
        let closed = reading(&[(1, 6, "SELECT a"), (2, 9, "UPDATE b"), (3, 1, "INSERT c")]);
        let ran = closed.since(&opened);
        assert_eq!(
            ran,
            Ran(vec![("SELECT a".to_owned(), 2), ("INSERT c".to_owned(), 1)])
        );
        assert_eq!(ran.total(), 3);
    }

    /// A fragment sums every statement holding it, and one nothing holds is 0.
    #[test]
    fn fragment_sums_matching_statements_only() {
        let ran = Ran(vec![
            ("SELECT x FROM runner.leases".to_owned(), 2),
            ("UPDATE runner.leases SET y = $1".to_owned(), 1),
            ("SELECT z FROM fleet.events".to_owned(), 5),
        ]);
        assert_eq!(ran.calls_containing("runner.leases"), 3);
        assert_eq!(ran.calls_containing("vault.secrets"), 0);
    }
}
