//! Reading and ageing one row of any table the lease suites touch.
//!
//! Split from `fleet_fixtures.rs` at the file cap. Every column read and every
//! ageing write here goes through [`Row`], so a table's name and key are
//! spelled once.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by two test binaries, each using a subset"
)]

use af_core::id::Uuid7;
use jiff::Timestamp;
use sqlx::AssertSqlSafe;

use crate::support::Fixtures;

/// Far enough to put any deadline a claim, an issue or a renewal wrote behind
/// the database clock: each lies at most one lease lifetime ahead of it.
pub(crate) const LAPSE: std::time::Duration = af_core::timing::LEASE_TTL.saturating_mul(2);

/// One row a test reads a column of, or ages: the table it lives in and the
/// key that names it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Row<'a> {
    /// A `runner.runners` row, by runner id.
    Runner(&'a Uuid7),
    /// A `runner.leases` row, by lease id.
    Lease(&'a Uuid7),
    /// A fleet's `runner.fleet_slots` row, by fleet id.
    Slot(&'a Uuid7),
    /// A runner's `runner.lifetime_counters` row, by runner id.
    Counters(&'a Uuid7),
    /// A fleet's `fleet.sessions` row, by fleet id.
    Session(&'a Uuid7),
    /// A `fleet.admissions` row, by its id, which is the logical event id.
    Admission(&'a Uuid7),
    /// An `authoring.attempts` row, by attempt id.
    Attempt(&'a Uuid7),
    /// A `fleet.events` row, by fleet id and event id.
    Event(&'a Uuid7, &'a Uuid7),
    /// A `billing.usage_lines` row, by event id and line number.
    Line(&'a Uuid7, &'a str),
}

/// The second key a two-key predicate reads.
#[derive(Debug, Clone, Copy)]
enum Second<'a> {
    /// An identifier, bound as a `uuid`.
    Id(&'a Uuid7),
    /// A line number, spelled as text and cast by the predicate.
    Seq(&'a str),
}

impl<'a> Row<'a> {
    /// The table, the predicate that finds the row, and its keys: the first
    /// always, the second only for a two-key predicate.
    const fn locate(self) -> (&'static str, &'static str, &'a Uuid7, Option<Second<'a>>) {
        match self {
            Self::Runner(id) => ("runner.runners", "id = $1", id, None),
            Self::Lease(id) => ("runner.leases", "id = $1", id, None),
            Self::Slot(fleet) => ("runner.fleet_slots", "fleet_id = $1", fleet, None),
            Self::Counters(runner) => ("runner.lifetime_counters", "runner_id = $1", runner, None),
            Self::Session(fleet) => ("fleet.sessions", "fleet_id = $1", fleet, None),
            Self::Admission(id) => ("fleet.admissions", "id = $1", id, None),
            Self::Attempt(id) => ("authoring.attempts", "id = $1", id, None),
            Self::Event(fleet, event) => (
                "fleet.events",
                "fleet_id = $1 AND event_id = $2",
                fleet,
                Some(Second::Id(event)),
            ),
            Self::Line(event, seq) => (
                "billing.usage_lines",
                "event_id = $1 AND seq = $2::bigint",
                event,
                Some(Second::Seq(seq)),
            ),
        }
    }
}

impl Fixtures {
    /// One column of `row`, as text; `None` for no row or a NULL.
    ///
    /// For text, integer and boolean columns, compared as the stored spelling.
    /// An instant is read through [`Fixtures::instant`] instead.
    pub(crate) async fn text(&self, row: Row<'_>, column: &str) -> Option<String> {
        self.cell::<String>(row, &format!("{column}::text")).await
    }

    /// One timestamp column of `row`, decoded; `None` for no row or a NULL.
    pub(crate) async fn instant(&self, row: Row<'_>, column: &str) -> Option<Timestamp> {
        self.cell::<jiff_sqlx::Timestamp>(row, column)
            .await
            .map(jiff_sqlx::Timestamp::to_jiff)
    }

    /// One column of `row`, decoded as `T`; `None` for no row or a NULL.
    pub(crate) async fn value<T>(&self, row: Row<'_>, column: &str) -> Option<T>
    where
        T: for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + Unpin,
    {
        self.cell(row, column).await
    }

    async fn cell<T>(&self, row: Row<'_>, expression: &str) -> Option<T>
    where
        T: for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + Unpin,
    {
        let (table, predicate, first, second) = row.locate();
        let statement = AssertSqlSafe(format!(
            "SELECT {expression} FROM {table} WHERE {predicate}"
        ));
        let query = sqlx::query_scalar::<_, Option<T>>(statement).bind(first);
        let query = match second {
            Some(Second::Id(id)) => query.bind(id),
            Some(Second::Seq(seq)) => query.bind(seq),
            None => query,
        };
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        query
            .fetch_optional(&mut *connection)
            .await
            .expect("the column read must run")
            .flatten()
    }

    /// Moves one timestamp column of `row` back by `by`, as that much time
    /// passing would have.
    ///
    /// Every deadline the code under test checks is read on the database's
    /// clock, so a test reaches a lapsed lease, an expired hold, a silent
    /// runner or a run at its hard cap by ageing the row it reads, never by
    /// handing the code a future instant.
    pub(crate) async fn age(&self, row: Row<'_>, column: &str, by: std::time::Duration) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        age_on(&mut connection, row, column, by).await;
    }
}

impl Fixtures {
    /// Lapses both deadlines a holder that stopped renewing leaves behind: its
    /// lease's and its fleet's slot's.
    pub(crate) async fn lapse_holder(&self, lease: &Uuid7, fleet: &Uuid7) {
        self.age(Row::Lease(lease), "lease_expires_at", LAPSE).await;
        self.age(Row::Slot(fleet), "lease_expires_at", LAPSE).await;
    }
}

/// [`Fixtures::age`], on a connection the caller holds: for a test that ages a
/// row inside the transaction already locking it.
pub(crate) async fn age_on(
    connection: &mut sqlx::PgConnection,
    row: Row<'_>,
    column: &str,
    by: std::time::Duration,
) {
    let (table, predicate, first, second) = row.locate();
    let statement = AssertSqlSafe(format!(
        "UPDATE {table} SET {column} = {column} - ${shift}::interval WHERE {predicate}",
        shift = if second.is_some() { 3 } else { 2 }
    ));
    let query = sqlx::query(statement).bind(first);
    let query = match second {
        Some(Second::Id(id)) => query.bind(id),
        Some(Second::Seq(seq)) => query.bind(seq),
        None => query,
    };
    let aged = query
        .bind(by)
        .execute(&mut *connection)
        .await
        .expect("the ageing write must run");
    assert_eq!(
        aged.rows_affected(),
        1,
        "ageing {column} needs exactly one {table} row"
    );
}
