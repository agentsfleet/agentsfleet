//! Reads and seeds the §3 suites need: the wallet a settle draws on, the
//! catalogue that gives a run a price at all, and the rows it leaves behind.
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

/// The catalogue row a seeded rate is written under.
///
/// Fixed and SHARED. These suites price exactly one `(provider, model)` pair,
/// so the `ON CONFLICT (provider, model_id)` arm below always resolves to THIS
/// row — Postgres tests the arbiter index before attempting the insert, so the
/// primary key never enters into it and one shared database is safe.
///
/// A suite that starts pricing a SECOND pair needs a second constant. Reusing
/// this one would put two models under one primary key, and the arbiter arm
/// would not catch it because the pair it arbitrates on differs.
const FIXTURE_MODEL_ROW: &str = "0195b4ba-8d3a-7f01-8abc-000000000001";

/// Per-million-token rates a seeded catalogue row carries.
///
/// The magnitudes are Anthropic-shaped ($3 in, $15 out per million tokens, and
/// cached input at a tenth of fresh) rather than round numbers, because the
/// estimate floor prices only `ESTIMATE_FLOOR_INPUT_TOKENS` + output tokens —
/// 100 each — and a rate below 10,000 nanos per million tokens floors to ZERO
/// under `slice_charge`'s integer division. A test seeding a "nominal" rate of
/// 1 would look seeded and still be unpriceable.
const FIXTURE_INPUT_NANOS_PER_MTOK: i64 = 3_000_000_000;
const FIXTURE_CACHED_INPUT_NANOS_PER_MTOK: i64 = 300_000_000;
const FIXTURE_OUTPUT_NANOS_PER_MTOK: i64 = 15_000_000_000;

/// The context window a seeded catalogue row advertises.
///
/// Nothing under test reads it; the column is `NOT NULL`.
const FIXTURE_CONTEXT_CAP_TOKENS: i32 = 200_000;

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

    /// Gives `(provider, model)` a catalogue rate, so a run under platform
    /// posture is PRICEABLE.
    ///
    /// Needed by any test asserting the CREDITS gate. Without a catalogue row
    /// the estimate is `Estimate::Unpriceable`, whose floor is zero, and a zero
    /// balance covers zero — so the credits gate ADMITS and whichever gate runs
    /// next answers instead. That fail-open posture is deliberate production
    /// behaviour (an unpriced model must not strand a tenant), which is exactly
    /// why a test naming the credits refusal has to seed past it rather than
    /// assume the default reaches it.
    ///
    /// Idempotent on `(provider, model_id)`, matching the admin upsert.
    pub(crate) async fn seed_model_rate(&self, provider: &str, model: &str, now: i64) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(
            "INSERT INTO core.model_library
               (id, model_id, provider, context_cap_tokens,
                input_nanos_per_mtok, cached_input_nanos_per_mtok,
                output_nanos_per_mtok, created_at, updated_at)
             VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8)
             ON CONFLICT (provider, model_id) DO UPDATE
               SET input_nanos_per_mtok = EXCLUDED.input_nanos_per_mtok,
                   cached_input_nanos_per_mtok = EXCLUDED.cached_input_nanos_per_mtok,
                   output_nanos_per_mtok = EXCLUDED.output_nanos_per_mtok,
                   updated_at = EXCLUDED.updated_at",
        )
        .bind(FIXTURE_MODEL_ROW)
        .bind(model)
        .bind(provider)
        .bind(FIXTURE_CONTEXT_CAP_TOKENS)
        .bind(FIXTURE_INPUT_NANOS_PER_MTOK)
        .bind(FIXTURE_CACHED_INPUT_NANOS_PER_MTOK)
        .bind(FIXTURE_OUTPUT_NANOS_PER_MTOK)
        .bind(now)
        .execute(&mut *connection)
        .await
        .expect("the catalogue seed must run");
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
