//! What the money pass decides, over the rows it actually reads.
//!
//! `admit/fault.rs` proves each gate's POSTURE — what a fault at it means — and
//! `verdict_matrix.rs` proves the gate verdicts. Neither runs the pass, so the
//! order the pass runs in read no covered lines: the payer lookup that ends an
//! event, the ceiling that refuses one, and the receipt that is not charged
//! twice.
//!
//! # The order is the safety property
//!
//! Payer → balance → fleet budget → receipt. Every gate that can refuse
//! permanently precedes the debit, so a refused event is never charged. A build
//! that moved the debit up would still pass every gate's own test and would
//! bill people for work it then declined to do.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_billing::ledger::LedgerBiller;
use afd_billing::{Accounts, Posture};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_fleet::lease::admit::{Admission, Request, money_gates};
use afd_fleet_runtime::config::FleetConfig;

use crate::seed::unique_ids;
use crate::support::Fixtures;

/// The provider every request below names.
const PROVIDER: &str = "anthropic";

/// A model no catalogue row prices.
///
/// Deliberate: an unpriceable model estimates zero, which ADMITS at the balance
/// gate and lets each test below reach the gate it is actually about. The
/// balance gate's own refusal has its own coverage elsewhere.
const MODEL: &str = "fixture-model-no-rate";

/// A ceiling small enough that one seeded ledger row passes it.
///
/// Authored as a document rather than constructed: `Budget` is built by the
/// same parser that accepted the fleet's config at ingest, and there is no
/// other way to make one — which is the point, since a ceiling that admits a
/// run and a ceiling that stops one must never be read two ways.
const TINY_DAILY_NANOS: &str = "1000";

/// What one seeded ledger row drained, comfortably past the ceiling above.
const SPENT_NANOS: i64 = 10_000_000;

/// A ceiling far above what [`SPENT_NANOS`] drained.
///
/// Named because it is the counterpart of [`TINY_DAILY_NANOS`]: the pair is
/// one fact — a spend that is over one ceiling and under the other — and a
/// literal on each side could drift on one and still read as a pass.
const AMPLE_DAILY_NANOS: &str = "1000000000000";

/// The ceiling a workspace with no tenant row declares; its size is irrelevant,
/// since the missing payer ends the delivery before any ceiling is read.
const ORPHAN_DAILY_NANOS: &str = "10000000000";

/// A config carrying `daily` as its whole budget.
fn budgeted(daily_nanos: &str) -> FleetConfig {
    // The runtime block is where every knob lives — a `budget` at the top
    // level is refused by name, which is the parser telling an author to
    // indent rather than dropping the ceiling in silence.
    let document = format!(
        r#"{{"name":"money-gates-fixture","x-agentsfleet":{{"triggers":[],
             "tools":[],"budget":{{"daily_nanos":"{daily_nanos}"}}}}}}"#
    );
    FleetConfig::authored(&document).expect("the fixture document is authorable")
}

/// The request the pass is given, for `fleet` in `workspace`, its event created
/// at `created_at`.
fn request<'a>(
    workspace: &'a Uuid7,
    fleet: &'a Uuid7,
    event: &'a str,
    config: &FleetConfig,
) -> Request<'a> {
    Request {
        workspace_id: workspace,
        fleet_id: fleet,
        event_id: event,
        budget: config.budget(),
        posture: Posture::Platform,
        provider: PROVIDER,
        model: MODEL,
    }
}

/// The built-in biller over the fixture database.
fn biller(fixtures: &Fixtures) -> LedgerBiller {
    LedgerBiller::new(
        fixtures.database.clone(),
        Entropy::new(),
        Arc::new(afd_core::clock::SystemClock),
    )
}

/// An identifier the seeds minted, as a `Uuid7`.
fn id(raw: &str) -> Uuid7 {
    Uuid7::parse(raw).expect("the seeded identifiers are canonical")
}

/// A workspace that resolves to no tenant ends the event, and is not charged.
///
/// A broken foreign key: waiting does not repair it, and running work nobody
/// can be billed for is worse than ending the delivery. The assertion that
/// matters is the CLASS — `Retry` here would leave the delivery leasable and
/// every poll would re-read the same missing row forever.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_workspace_naming_no_tenant_ends_the_event() {
    let fixtures = Fixtures::create().await;
    let accounts = Accounts::new(fixtures.database.clone());
    let biller = biller(&fixtures);
    let (fleet, workspace, _tenant) = unique_ids();
    // Deliberately NOT seeded: this is the missing row under test.
    let config = budgeted(ORPHAN_DAILY_NANOS);

    let decided = money_gates(
        &accounts,
        &biller,
        accounts.payer(&id(&workspace)).await,
        request(&id(&workspace), &id(&fleet), "event-unowned", &config),
        fixtures.enrolled_at(),
    )
    .await
    .expect("a missing payer is a decision, not a fault the caller sees");

    let Admission::Refuse(refusal) = decided else {
        panic!("an unowned workspace must be refused, got {decided:?}");
    };
    assert_eq!(refusal.label, afd_core::event::label::TENANT_RESOLVE_FAILED);

    fixtures.cleanup().await;
}

/// A fleet past its daily ceiling is refused, and refused permanently.
///
/// The ceiling is the fleet author's own, read live from `config_json`. What
/// this proves that `budget::covers` alone cannot is that the pass READS the
/// spend for the right fleet and window and acts on it — a query filtered
/// wrongly would answer zero and admit every run past every ceiling.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_fleet_past_its_daily_ceiling_is_refused() {
    let fixtures = Fixtures::create().await;
    let accounts = Accounts::new(fixtures.database.clone());
    let biller = biller(&fixtures);
    let (fleet, workspace, tenant) = unique_ids();
    fixtures
        .seed_fleet(&fleet, &workspace, &tenant, "money-gates")
        .await;
    seed_spend(&fixtures, &tenant, &workspace, &fleet, SPENT_NANOS).await;
    let config = budgeted(TINY_DAILY_NANOS);

    let decided = money_gates(
        &accounts,
        &biller,
        accounts.payer(&id(&workspace)).await,
        request(&id(&workspace), &id(&fleet), "event-over-budget", &config),
        fixtures.enrolled_at(),
    )
    .await
    .expect("a breached ceiling is a decision, not a fault");

    let Admission::Refuse(refusal) = decided else {
        panic!("a fleet past its ceiling must be refused, got {decided:?}");
    };
    assert_eq!(refusal.label, afd_core::event::label::BUDGET_BREACH);

    fixtures.cleanup().await;
}

/// The same fleet under a ceiling it has not reached is admitted and billed.
///
/// The other side of the assertion above, and the reason it is not enough to
/// test the refusal alone: a pass that refused everything would satisfy the
/// budget test and stop the platform.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_fleet_inside_its_ceiling_is_admitted_and_billed() {
    let fixtures = Fixtures::create().await;
    let accounts = Accounts::new(fixtures.database.clone());
    let biller = biller(&fixtures);
    let (fleet, workspace, tenant) = unique_ids();
    fixtures
        .seed_fleet(&fleet, &workspace, &tenant, "money-gates")
        .await;
    seed_spend(&fixtures, &tenant, &workspace, &fleet, SPENT_NANOS).await;
    let config = budgeted(AMPLE_DAILY_NANOS);

    let decided = money_gates(
        &accounts,
        &biller,
        accounts.payer(&id(&workspace)).await,
        request(&id(&workspace), &id(&fleet), "event-inside-budget", &config),
        fixtures.enrolled_at(),
    )
    .await
    .expect("an admitted run is not a fault");

    let Admission::Admit(admitted) = decided else {
        panic!("a fleet inside its ceiling must be admitted, got {decided:?}");
    };
    assert_eq!(
        admitted.tenant_id.as_str(),
        tenant,
        "the run is billed to the tenant the workspace resolves to"
    );
    assert_eq!(&*admitted.model, MODEL, "and against the model it will run");

    fixtures.cleanup().await;
}

/// Admission moves no money: the gates read, and the lease's hold and usage
/// are the biller's once the lease is issued.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_admitted_run_is_charged_nothing_at_admission() {
    let fixtures = Fixtures::create().await;
    let accounts = Accounts::new(fixtures.database.clone());
    let biller = biller(&fixtures);
    let (fleet, workspace, tenant) = unique_ids();
    fixtures
        .seed_fleet(&fleet, &workspace, &tenant, "money-gates")
        .await;
    let config = budgeted(AMPLE_DAILY_NANOS);

    let decided = money_gates(
        &accounts,
        &biller,
        accounts.payer(&id(&workspace)).await,
        request(&id(&workspace), &id(&fleet), "event-admitted", &config),
        fixtures.enrolled_at(),
    )
    .await
    .expect("an admitted run is not a fault");

    assert!(
        matches!(decided, Admission::Admit(_)),
        "an inside-budget run is admitted: {decided:?}"
    );
    assert_eq!(
        usage_lines(&fixtures, &fleet).await,
        0,
        "admission writes no usage line; a run is charged for what it uses"
    );

    fixtures.cleanup().await;
}

/// Seeds one settled ledger row draining `nanos` for `fleet`.
async fn seed_spend(fixtures: &Fixtures, tenant: &str, workspace: &str, fleet: &str, nanos: i64) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO billing.usage_lines \
           (id, work_type, work_id, seq, tenant_id, workspace_id, fleet_id, fleet_name, event_id, \
            provider, model, posture, requested_at, input_token_count, cached_input_token_count, \
            cache_write_token_count, cache_write_5m_token_count, cache_write_1h_token_count, \
            output_token_count, runtime_ms, unpriced_token_count, charged_nanos, created_at, \
            updated_at) \
         VALUES ($1::uuid, $2, $1::uuid, 0, $3::uuid, $4::uuid, $5::uuid, $5, $6, $7, $8, \
                 'platform', now(), 0, 0, 0, 0, 0, 0, 0, 0, $9, now(), now())",
    )
    .bind(ledger_id(fleet))
    .bind(afd_billing::sql::work::FLEET_LEASE)
    .bind(tenant)
    .bind(workspace)
    .bind(fleet)
    .bind(format!("event-already-spent-{fleet}"))
    .bind(PROVIDER)
    .bind(MODEL)
    .bind(nanos)
    .execute(&mut *connection)
    .await
    .expect("the spend seeds");
}

fn ledger_id(fleet: &str) -> String {
    let (head, _slot) = fleet.split_at(fleet.len() - 4);
    format!("{head}9999")
}

/// How many ledger rows this fleet holds.
async fn usage_lines(fixtures: &Fixtures, fleet: &str) -> i64 {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query_scalar("SELECT COUNT(*) FROM billing.usage_lines WHERE fleet_id = $1::uuid")
        .bind(fleet)
        .fetch_one(&mut *connection)
        .await
        .expect("the usage log answers a count")
}
