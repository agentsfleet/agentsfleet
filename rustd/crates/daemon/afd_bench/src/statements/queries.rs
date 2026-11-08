//! Each statement's calls, by query, so a window's cost can be split by path.
//!
//! [`super::read`] answers one sum. Splitting it needs the rows behind the sum:
//! `pg_stat_statements` keeps one row per query id (and per top-level flag,
//! which this folds together), with the normalized text of its first run.
//! Transaction control is left out, as the sum leaves it out, and so are the
//! counter's own reads, which name the statistics views they read.

use std::collections::BTreeMap;

use afd_db::Db;
use sqlx::FromRow as _;

use super::unreadable;
use crate::datastores::postgres_count;
use crate::error::Result;

/// Every product statement's calls in the lane's database, one row per query.
///
/// Over the simple protocol, like the sum: a prepared read's Parse would
/// commit a transaction of its own inside the window it measures.
const READ: &str = "SELECT queryid, sum(calls)::bigint, min(query) FROM pg_stat_statements \
       WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
         AND query !~* '^\\s*(BEGIN|START TRANSACTION|COMMIT|END|ROLLBACK|SAVEPOINT|RELEASE|ABORT)\\y' \
         AND query !~ '(pg_stat_|pg_database|pg_extension)' \
       GROUP BY queryid";

/// [`READ`]'s one row, by position: the query id, its calls, its text.
#[derive(Debug, sqlx::FromRow)]
struct QueryRow(i64, i64, String);

/// A call count, as a refusal names it.
const CALLS_FIELD: &str = "pg_stat_statements.calls";

/// The rows at one instant: calls and text by query id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryReading {
    calls: BTreeMap<i64, (u64, String)>,
}

/// One query's calls inside a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryCost {
    /// The normalized text Postgres recorded.
    pub query: String,
    /// Times it ran in the window.
    pub calls: u64,
}

impl QueryReading {
    /// A reading built from rows, for a test that needs no database.
    #[cfg(test)]
    pub(crate) fn of(rows: &[(i64, u64, &str)]) -> Self {
        Self {
            calls: rows
                .iter()
                .map(|(id, calls, query)| (*id, (*calls, (*query).to_owned())))
                .collect(),
        }
    }

    /// Every query that ran between `earlier` and this reading, with its calls.
    ///
    /// A query first seen in this reading ran its whole count in the window.
    #[must_use]
    pub fn since(&self, earlier: &Self) -> Vec<QueryCost> {
        self.calls
            .iter()
            .filter_map(|(id, (calls, query))| {
                let before = earlier.calls.get(id).map_or(0, |(calls, _)| *calls);
                let ran = calls.saturating_sub(before);
                (ran > 0).then(|| QueryCost {
                    query: query.clone(),
                    calls: ran,
                })
            })
            .collect()
    }
}

/// Read every query's calls for the lane's database.
///
/// No flush first: `pg_stat_statements` counts a call as its statement ends,
/// unlike the commit tally [`super::read`] has to flush for.
///
/// # Errors
///
/// `DatabaseUnavailable` when a connection will not open, and
/// `StatementsUnreadable` when the view will not answer.
pub async fn read_queries(database: &Db) -> Result<QueryReading> {
    let mut connection = database.acquire().await?;
    let rows = sqlx::raw_sql(READ)
        .fetch_all(&mut *connection)
        .await
        .map_err(unreadable)?;
    let mut calls = BTreeMap::new();
    for row in rows {
        let QueryRow(id, count, query) = QueryRow::from_row(&row).map_err(unreadable)?;
        calls.insert(id, (postgres_count(count, CALLS_FIELD)?, query));
    }
    Ok(QueryReading { calls })
}
