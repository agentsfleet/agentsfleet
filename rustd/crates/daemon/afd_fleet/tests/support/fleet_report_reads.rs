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
use af_core::id::Uuid7;
use afd_db::test_util::{ModelSeed, mint_id};

/// The fixture model routes through Anthropic Messages.
const MESSAGES_INFERENCE: &str =
    r#"{"api":"messages","base_url":"https://api.anthropic.com/v1","max_output_tokens":8192}"#;

/// Gives a tenant a balance, or resets the one it has.
const UPSERT_WALLET: &str = "\
INSERT INTO billing.wallets (tenant_id, balance_nanos, exhausted_at, created_at, updated_at)
VALUES ($1, $2, NULL, now(), now())
ON CONFLICT (tenant_id) DO UPDATE
  SET balance_nanos = EXCLUDED.balance_nanos, updated_at = now()";

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
    pub(crate) async fn seed_wallet(&self, tenant: &Uuid7, nanos: i64) {
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
        // Concurrent inserts may check the primary key before resolving the
        // provider/model conflict. Each attempt needs its own proposed key.
        let row = mint_id();
        ModelSeed::new(&row, provider, model, FIXTURE_CONTEXT_CAP_TOKENS)
            .rates(
                FIXTURE_INPUT_NANOS_PER_MTOK,
                FIXTURE_CACHED_INPUT_NANOS_PER_MTOK,
                FIXTURE_OUTPUT_NANOS_PER_MTOK,
            )
            .inference(MESSAGES_INFERENCE)
            .insert(&mut *connection)
            .await
            .expect("the catalogue seed must run");
    }

    /// A tenant's current balance, or `None` when it has no wallet row.
    pub(crate) async fn balance(&self, tenant: &Uuid7) -> Option<i64> {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query_scalar("SELECT balance_nanos FROM billing.wallets WHERE tenant_id = $1")
            .bind(tenant)
            .fetch_optional(&mut *connection)
            .await
            .expect("the balance read must run")
    }

    /// How many usage lines one event's runs have logged.
    ///
    /// One per slice that measured something, and the same count however many
    /// times a report is re-sent: a line is stored once per `(work, seq)`.
    pub(crate) async fn ledger_rows(&self, event: &Uuid7) -> i64 {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query_scalar("SELECT count(*) FROM billing.usage_lines WHERE event_id = $1")
            .bind(event)
            .fetch_one(&mut *connection)
            .await
            .expect("the ledger count must run")
    }

    /// Whether the admission can still be claimed after a report attempt.
    pub(crate) async fn admission_settled(&self, event: &Uuid7) -> bool {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query_scalar("SELECT settled_at IS NOT NULL FROM fleet.admissions WHERE id = $1")
            .bind(event)
            .fetch_one(&mut *connection)
            .await
            .expect("the admission settlement read must run")
    }

    /// Ends an event out of band, as an earlier delivery of the same logical
    /// event already did.
    ///
    /// Exercises a report whose narrative row ended before the report arrived.
    /// The terminal write must preserve that earlier outcome.
    pub(crate) async fn end_event(&self, fleet: &Uuid7, event: &Uuid7, status: &str) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(
            "UPDATE fleet.events SET status = $3::text \
             WHERE fleet_id = $1 AND event_id = $2",
        )
        .bind(fleet)
        .bind(event)
        .bind(status)
        .execute(&mut *connection)
        .await
        .expect("the event write must run");
    }
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn model_rate_seeds_keep_distinct_models_and_concurrent_retries() {
    let fixtures = Fixtures::create_isolated().await;
    let first = "fixture-priced-first";
    let second = "fixture-priced-second";
    tokio::join!(
        fixtures.seed_model_rate("anthropic", first),
        fixtures.seed_model_rate("anthropic", first),
        fixtures.seed_model_rate("anthropic", second),
    );
    let mut connection = fixtures.database.acquire().await.expect("fixture read");
    let rows: Vec<(String, i64, i64, i64)> = sqlx::query_as(
        "SELECT m.model_name, p.input_nanos_per_mtok, p.cached_input_nanos_per_mtok, \
         p.output_nanos_per_mtok FROM model.platform_models m \
         JOIN model.platform_model_prices p ON p.platform_model_id = m.id \
         WHERE m.provider = $1 ORDER BY m.model_name",
    )
    .bind("anthropic")
    .fetch_all(&mut *connection)
    .await
    .expect("priced rows");
    assert_eq!(
        rows,
        vec![
            (first.into(), 3_000_000_000, 300_000_000, 15_000_000_000),
            (second.into(), 3_000_000_000, 300_000_000, 15_000_000_000),
        ]
    );
}
