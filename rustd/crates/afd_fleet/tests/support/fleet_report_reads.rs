//! Reads and seeds the §3 suites need: the wallet a settle draws on, and the
//! rows it leaves behind.
//!
//! Separate from `fleet_lease_reads.rs` because the tables are different and
//! the question is: those helpers ask what a LEASE looks like, these ask what a
//! run COST. A suite proving row parity on the report needs both, and merging
//! them would put every lease suite's compile behind the billing schema.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use sqlx::{AssertSqlSafe, Row as _};

use crate::support::Fixtures;

/// The `grant_source` a seeded wallet records.
///
/// A fixture provenance rather than a real one: nothing in these suites reads
/// it, and a spelling that looked like a production grant source would be a
/// test row indistinguishable from a real top-up if one ever leaked.
const FIXTURE_GRANT: &str = "fixture:seed";

impl Fixtures {
    /// Gives a tenant a credit pool of `nanos`.
    ///
    /// Idempotent, so a test may top up mid-run to prove the gate re-reads
    /// rather than caching. A tenant with NO wallet row is a different case
    /// entirely — it is admitted — so a suite proving the exhausted refusal has
    /// to seed a row holding zero, not skip the seed.
    pub(crate) async fn seed_wallet(&self, tenant: &str, nanos: i64, now: i64) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(
            "INSERT INTO billing.tenant_wallet
               (tenant_id, balance_nanos, grant_source, created_at, updated_at)
             VALUES ($1::uuid, $2, $3, $4, $4)
             ON CONFLICT (tenant_id) DO UPDATE
               SET balance_nanos = EXCLUDED.balance_nanos, updated_at = EXCLUDED.updated_at",
        )
        .bind(tenant)
        .bind(nanos)
        .bind(FIXTURE_GRANT)
        .bind(now)
        .execute(&mut *connection)
        .await
        .expect("the wallet seed must run");
    }

    /// A tenant's current balance, or `None` when it has no wallet row.
    pub(crate) async fn balance(&self, tenant: &str) -> Option<i64> {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("SELECT balance_nanos FROM billing.tenant_wallet WHERE tenant_id = $1::uuid")
            .bind(tenant)
            .fetch_optional(&mut *connection)
            .await
            .expect("the balance read must run")
            .map(|row| row.try_get(0).expect("the balance is a bigint"))
    }

    /// How many ledger rows one event has, across every charge type.
    ///
    /// The number Dimension 3.3 pins: two, and two however many times the
    /// report is re-sent.
    pub(crate) async fn ledger_rows(&self, event: &str) -> i64 {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("SELECT count(*) FROM billing.usage_ledger WHERE event_id = $1")
            .bind(event)
            .fetch_one(&mut *connection)
            .await
            .expect("the ledger count must run")
            .try_get(0)
            .expect("a count is a bigint")
    }

    /// One column of one event's ledger row for `charge_type`, as text.
    pub(crate) async fn ledger_column(
        &self,
        event: &str,
        charge_type: &str,
        column: &str,
    ) -> Option<String> {
        let statement = AssertSqlSafe(format!(
            "SELECT {column}::text FROM billing.usage_ledger \
             WHERE event_id = $1 AND charge_type = $2"
        ));
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(statement)
            .bind(event)
            .bind(charge_type)
            .fetch_optional(&mut *connection)
            .await
            .expect("the ledger read must run")
            .map(|row| row.try_get(0).expect("the column must be readable as text"))
    }

    /// One column of a `core.fleet_events` row, as text.
    pub(crate) async fn event_column(
        &self,
        fleet: &str,
        event: &str,
        column: &str,
    ) -> Option<String> {
        let statement = AssertSqlSafe(format!(
            "SELECT {column}::text FROM core.fleet_events \
             WHERE fleet_id = $1::uuid AND event_id = $2"
        ));
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(statement)
            .bind(fleet)
            .bind(event)
            .fetch_optional(&mut *connection)
            .await
            .expect("the event read must run")
            .map(|row| row.try_get(0).expect("the column must be readable as text"))
    }

    /// One column of a runner's lifetime counters, as text.
    pub(crate) async fn counter_column(&self, runner: &str, column: &str) -> Option<String> {
        let statement = AssertSqlSafe(format!(
            "SELECT {column}::text FROM fleet.runner_lifetime_counters WHERE runner_id = $1::uuid"
        ));
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(statement)
            .bind(runner)
            .fetch_optional(&mut *connection)
            .await
            .expect("the counter read must run")
            .map(|row| row.try_get(0).expect("the column must be readable as text"))
    }

    /// Stands a lease's `created_at` back in time, so the max-runtime clamp is
    /// reachable without a test that waits twelve hours.
    ///
    /// The clamp is `LEAST(now + LEASE_TTL_MS, created_at + MAX_RUNTIME_MS)`,
    /// and both operands come from the row — so moving `created_at` is the only
    /// way to exercise the second arm. Writing it directly rather than through
    /// a store verb because no verb in this crate back-dates a lease, and
    /// inventing one to serve a test would put a write path nothing ships into
    /// the shipping crate.
    pub(crate) async fn backdate_lease(&self, lease: &str, created_at: i64) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("UPDATE fleet.runner_leases SET created_at = $2 WHERE id = $1::uuid")
            .bind(lease)
            .bind(created_at)
            .execute(&mut *connection)
            .await
            .expect("the backdate must run");
    }
}
