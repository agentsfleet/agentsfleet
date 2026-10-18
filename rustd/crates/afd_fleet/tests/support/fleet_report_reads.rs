//! Reads and seeds the §3 suites need: the wallet a settle draws on, the
//! catalogue that gives a run a price at all, and the rows it leaves behind.
//!
//! Separate from `fleet_lease_reads.rs` because the tables are different and
//! so is the question: those helpers stage a LEASE, these ask what a run COST.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use crate::support::Fixtures;
use afd_db::test_util::ModelSeed;

/// Gives a tenant a balance, or resets the one it has.
const UPSERT_WALLET: &str = "\
INSERT INTO billing.wallets (tenant_id, balance_nanos, exhausted_at, created_at, updated_at)
VALUES ($1::uuid, $2, NULL, now(), now())
ON CONFLICT (tenant_id) DO UPDATE
  SET balance_nanos = EXCLUDED.balance_nanos, updated_at = now()";

/// The catalogue row a seeded rate is written under.
///
/// Fixed and SHARED. These suites price exactly one `(provider, model)` pair,
/// so the `ON CONFLICT (provider, model_name)` arm always resolves to THIS
/// row: Postgres tests the arbiter index before attempting the insert, so the
/// primary key never enters into it and one shared database is safe. A suite
/// that prices a SECOND pair needs a second constant.
const FIXTURE_MODEL_ROW: &str = "0195b4ba-8d3a-7f01-8abc-000000000001";

/// Per-million-token rates a seeded catalogue row carries.
///
/// Anthropic-shaped ($3 in, $15 out per million tokens, cached input at a tenth
/// of fresh) rather than round numbers, because the estimate floor prices only
/// `ESTIMATE_FLOOR_INPUT_TOKENS` + output tokens — 100 each — and a rate below
/// 10,000 nanos per million tokens floors to ZERO under integer division.
const FIXTURE_INPUT_NANOS_PER_MTOK: i64 = 3_000_000_000;
const FIXTURE_CACHED_INPUT_NANOS_PER_MTOK: i64 = 300_000_000;
const FIXTURE_OUTPUT_NANOS_PER_MTOK: i64 = 15_000_000_000;

/// The context window a seeded catalogue row advertises; nothing reads it.
const FIXTURE_CONTEXT_CAP_TOKENS: i32 = 200_000;

impl Fixtures {
    /// Gives a tenant a credit pool of `nanos`.
    ///
    /// Idempotent, so a test may top up mid-run to prove the gate re-reads
    /// rather than caching. A tenant with NO wallet row is a different case
    /// entirely — it is admitted — so a suite proving the exhausted refusal has
    /// to seed a row holding zero, not skip the seed.
    pub(crate) async fn seed_wallet(&self, tenant: &str, nanos: i64) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(UPSERT_WALLET)
            .bind(tenant)
            .bind(nanos)
            .execute(&mut *connection)
            .await
            .expect("the wallet seed must run");
    }

    /// Gives `(provider, model)` a catalogue rate, so a run under platform
    /// posture is PRICEABLE.
    ///
    /// Without a catalogue row the estimate is unpriceable, whose floor is
    /// zero, and a zero balance covers zero — so the credits gate ADMITS. That
    /// fail-open posture is deliberate production behaviour, which is why a
    /// test naming the credits refusal seeds past it.
    pub(crate) async fn seed_model_rate(&self, provider: &str, model: &str) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        ModelSeed::new(
            FIXTURE_MODEL_ROW,
            provider,
            model,
            FIXTURE_CONTEXT_CAP_TOKENS,
        )
        .rates(
            FIXTURE_INPUT_NANOS_PER_MTOK,
            FIXTURE_CACHED_INPUT_NANOS_PER_MTOK,
            FIXTURE_OUTPUT_NANOS_PER_MTOK,
        )
        .insert(&mut *connection)
        .await
        .expect("the catalogue seed must run");
    }

    /// A tenant's current balance, or `None` when it has no wallet row.
    pub(crate) async fn balance(&self, tenant: &str) -> Option<i64> {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query_scalar("SELECT balance_nanos FROM billing.wallets WHERE tenant_id = $1::uuid")
            .bind(tenant)
            .fetch_optional(&mut *connection)
            .await
            .expect("the balance read must run")
    }

    /// How many charge rows one event has, across every charge type.
    ///
    /// The number Dimension 3.3 pins: two, and two however many times the
    /// report is re-sent.
    pub(crate) async fn ledger_rows(&self, event: &str) -> i64 {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query_scalar("SELECT count(*) FROM billing.fleet_charges WHERE event_id = $1")
            .bind(event)
            .fetch_one(&mut *connection)
            .await
            .expect("the ledger count must run")
    }

    /// Ends an event out of band, as an earlier delivery of the same logical
    /// event already did.
    ///
    /// One logical event can sit on two stream entries — a replayed admission
    /// puts it there — so a lease can legitimately be `active` over an event
    /// whose row is already terminal. The terminal write is guarded on
    /// `received` precisely for that case.
    pub(crate) async fn end_event(&self, fleet: &str, event: &str, status: &str) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(
            "UPDATE fleet.events SET status = $3::text \
             WHERE fleet_id = $1::uuid AND event_id = $2",
        )
        .bind(fleet)
        .bind(event)
        .bind(status)
        .execute(&mut *connection)
        .await
        .expect("the event write must run");
    }
}
