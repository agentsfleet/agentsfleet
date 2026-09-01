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

use afd_billing::{Accounts, Posture};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_fleet::lease::Delivery;
use afd_fleet::lease::admit::{Admission, Request, money_gates};
use afd_fleet_runtime::config::FleetConfig;

use crate::requests::ENROLLED_AT;
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
const TINY_DAILY_DOLLARS: &str = "0.000001";

/// What one seeded ledger row drained, comfortably past the ceiling above.
const SPENT_NANOS: i64 = 10_000_000;

/// A ceiling far above what [`SPENT_NANOS`] drained.
///
/// Named because it is the counterpart of [`TINY_DAILY_DOLLARS`]: the pair is
/// one fact — a spend that is over one ceiling and under the other — and a
/// literal on each side could drift on one and still read as a pass.
const AMPLE_DAILY_DOLLARS: &str = "1000";

/// A config carrying `daily` as its whole budget.
fn budgeted(daily_dollars: &str) -> FleetConfig {
    // The runtime block is where every knob lives — a `budget` at the top
    // level is refused by name, which is the parser telling an author to
    // indent rather than dropping the ceiling in silence.
    let document = format!(
        r#"{{"name":"money-gates-fixture","x-agentsfleet":{{"triggers":[{{"type":"api"}}],
             "tools":[],"budget":{{"daily_dollars":{daily_dollars}}}}}}}"#
    );
    FleetConfig::authored(&document).expect("the fixture document is authorable")
}

/// The request the pass is given, for `fleet` in `workspace`.
fn request<'a>(
    workspace: &'a Uuid7,
    fleet: &'a Uuid7,
    event: &'a str,
    config: &FleetConfig,
    delivery: Delivery,
) -> Request<'a> {
    Request {
        workspace_id: workspace,
        fleet_id: fleet,
        event_id: event,
        event_created_at: UnixMillis::from_millis(ENROLLED_AT),
        budget: config.budget(),
        posture: Posture::Platform,
        provider: PROVIDER,
        model: MODEL,
        delivery,
    }
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
    let accounts = Accounts::new(fixtures.database.clone(), Entropy::new());
    let (fleet, workspace, _tenant) = unique_ids();
    // Deliberately NOT seeded: this is the missing row under test.
    let config = budgeted("10");

    let decided = money_gates(
        &accounts,
        request(
            &id(&workspace),
            &id(&fleet),
            "event-unowned",
            &config,
            Delivery::First,
        ),
        UnixMillis::from_millis(ENROLLED_AT),
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
    let accounts = Accounts::new(fixtures.database.clone(), Entropy::new());
    let (fleet, workspace, tenant) = unique_ids();
    fixtures
        .seed_fleet(&fleet, &workspace, &tenant, "money-gates", ENROLLED_AT)
        .await;
    seed_spend(&fixtures, &tenant, &workspace, &fleet, SPENT_NANOS).await;
    let config = budgeted(TINY_DAILY_DOLLARS);

    let decided = money_gates(
        &accounts,
        request(
            &id(&workspace),
            &id(&fleet),
            "event-over-budget",
            &config,
            Delivery::First,
        ),
        UnixMillis::from_millis(ENROLLED_AT),
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
    let accounts = Accounts::new(fixtures.database.clone(), Entropy::new());
    let (fleet, workspace, tenant) = unique_ids();
    fixtures
        .seed_fleet(&fleet, &workspace, &tenant, "money-gates", ENROLLED_AT)
        .await;
    seed_spend(&fixtures, &tenant, &workspace, &fleet, SPENT_NANOS).await;
    let config = budgeted(AMPLE_DAILY_DOLLARS);

    let decided = money_gates(
        &accounts,
        request(
            &id(&workspace),
            &id(&fleet),
            "event-inside-budget",
            &config,
            Delivery::First,
        ),
        UnixMillis::from_millis(ENROLLED_AT),
    )
    .await
    .expect("an admitted run is not a fault");

    let Admission::Admit(billed) = decided else {
        panic!("a fleet inside its ceiling must be admitted, got {decided:?}");
    };
    assert_eq!(
        billed.tenant_id.as_str(),
        tenant,
        "the run is billed to the tenant the workspace resolves to"
    );
    assert_eq!(&*billed.model, MODEL, "and against the model it will run");

    fixtures.cleanup().await;
}

/// A redelivery is admitted without being charged again.
///
/// The receive charge is guarded by the delivery kind and NOT by the ledger's
/// own replay guard — the balance drain is not idempotent the way the row is —
/// so a second delivery charged again would silently double-bill every retry
/// the queue makes.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_redelivery_is_admitted_without_being_charged_again() {
    let fixtures = Fixtures::create().await;
    let accounts = Accounts::new(fixtures.database.clone(), Entropy::new());
    let (fleet, workspace, tenant) = unique_ids();
    fixtures
        .seed_fleet(&fleet, &workspace, &tenant, "money-gates", ENROLLED_AT)
        .await;
    let config = budgeted(AMPLE_DAILY_DOLLARS);
    let event = "event-redelivered";

    let decided = money_gates(
        &accounts,
        request(
            &id(&workspace),
            &id(&fleet),
            event,
            &config,
            Delivery::Repeat,
        ),
        UnixMillis::from_millis(ENROLLED_AT),
    )
    .await
    .expect("a redelivery is not a fault");

    assert!(
        matches!(decided, Admission::Admit(_)),
        "a redelivery still runs: {decided:?}"
    );
    assert_eq!(
        ledger_rows(&fixtures, &fleet).await,
        0,
        "a repeat delivery writes no receive charge — an earlier delivery paid"
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
        "INSERT INTO billing.usage_ledger \
           (id, tenant_id, workspace_id, fleet_id, event_id, charge_type, posture, model, \
            credit_deducted_nanos, event_created_at, created_at, last_charged_at) \
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5, 'receive', 'platform', $6, \
                 $7, $8, $8, $8)",
    )
    .bind(ledger_id(fleet))
    .bind(tenant)
    .bind(workspace)
    .bind(fleet)
    // `(event_id, charge_type)` is unique across the whole table, so this
    // is per-run for the same reason the row id is.
    .bind(format!("event-already-spent-{fleet}"))
    .bind(MODEL)
    .bind(nanos)
    .bind(ENROLLED_AT)
    .execute(&mut *connection)
    .await
    .expect("the spend seeds");
}

/// A ledger identifier unique to `fleet`'s run.
///
/// Derived from the fleet rather than from the amount: this lane shares one
/// database and `unique_ids` is what makes a run's rows its own, so an id built
/// from a constant collides with the same test's previous run and with every
/// sibling seeding the same amount. The last group is replaced, which keeps the
/// version nibble the table's own CHECK constraint reads.
fn ledger_id(fleet: &str) -> String {
    let (head, _slot) = fleet.split_at(fleet.len() - 4);
    format!("{head}9999")
}

/// How many ledger rows this fleet holds.
async fn ledger_rows(fixtures: &Fixtures, fleet: &str) -> i64 {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query_scalar("SELECT COUNT(*) FROM billing.usage_ledger WHERE fleet_id = $1::uuid")
        .bind(fleet)
        .fetch_one(&mut *connection)
        .await
        .expect("the ledger answers a count")
}
