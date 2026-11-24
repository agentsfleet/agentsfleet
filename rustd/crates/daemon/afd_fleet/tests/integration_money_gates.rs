//! What the money pass decides, over the rows it actually reads.
//!
//! `start_check/fault.rs` proves each gate's POSTURE — what a fault at it means — and
//! `verdict_matrix.rs` proves the gate verdicts. Neither runs the pass, so the
//! order the pass runs in read no covered lines: the payer lookup that ends an
//! event, the balance that refuses one, and the receipt that is not charged
//! twice.
//!
//! # The order is the safety property
//!
//! Payer → balance → receipt. Every gate that can refuse permanently precedes
//! the debit, so a refused event is never charged. The payer's balance is the
//! only ceiling a run has: a payer with nothing, or with no wallet at all, is
//! refused.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_billing::ledger::LedgerBiller;
use afd_billing::{Accounts, Posture};
use afd_fleet::run::start_check::{Request, StartCheck, money_gates};

use afd_db::test_util::mint_id;

use crate::seed::unique_ids;
use crate::support::Fixtures;

/// The provider every request below names.
const PROVIDER: &str = "anthropic";

/// A model no catalogue row prices.
///
/// An unpriceable model estimates zero, which a funded wallet covers, so each
/// test below reaches the gate it is actually about.
const MODEL: &str = "fixture-model-no-rate";

/// A model the suite prices, so a run on it has a floor an empty wallet cannot
/// cover.
const PRICED_MODEL: &str = "fixture-model-priced";

/// A wallet comfortably above any fixture run's floor.
pub(crate) const FUNDED_NANOS: i64 = 1_000_000_000_000;

/// The request the pass is given, for `fleet` in `workspace`, on `model`.
pub(crate) const fn request<'a>(
    workspace: &'a Uuid7,
    fleet: &'a Uuid7,
    event: &'a Uuid7,
) -> Request<'a> {
    priced_request(workspace, fleet, event, MODEL)
}

const fn priced_request<'a>(
    workspace: &'a Uuid7,
    fleet: &'a Uuid7,
    event: &'a Uuid7,
    model: &'a str,
) -> Request<'a> {
    Request {
        workspace_id: workspace,
        fleet_id: fleet,
        turn_id: event,
        posture: Posture::Platform,
        provider: PROVIDER,
        model,
    }
}

/// The built-in biller over the fixture database.
pub(crate) fn biller(fixtures: &Fixtures) -> LedgerBiller {
    LedgerBiller::new(fixtures.database.clone())
}

/// A workspace that resolves to no tenant ends the event, and is not charged.
///
/// A broken foreign key: waiting does not repair it, and running work nobody
/// can be billed for is worse than ending the delivery. The assertion that
/// matters is the CLASS — `Retry` here would leave the delivery leasable and
/// every poll would re-read the same missing row forever.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_workspace_naming_no_tenant_ends_the_event() {
    let fixtures = Fixtures::create().await;
    let accounts = Accounts::new(fixtures.database.clone());
    let biller = biller(&fixtures);
    let (fleet, workspace, _tenant) = unique_ids();
    // Deliberately NOT seeded: this is the missing row under test.
    let event = mint_id();

    let decided = money_gates(
        &accounts,
        &biller,
        Ok(None),
        request(&workspace, &fleet, &event),
        fixtures.enrolled_at(),
    )
    .await
    .expect("a missing payer is a decision, not a fault the caller sees");

    let StartCheck::Refuse(refusal) = decided else {
        panic!("an unowned workspace must be refused, got {decided:?}");
    };
    assert_eq!(refusal.label, af_core::turn::label::TENANT_RESOLVE_FAILED);

    fixtures.cleanup().await;
}

/// A funded payer's run is started, billed to the tenant its workspace names.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_funded_fleet_is_started_and_billed() {
    let fixtures = Fixtures::create().await;
    let accounts = Accounts::new(fixtures.database.clone());
    let biller = biller(&fixtures);
    let (fleet, workspace, tenant) = unique_ids();
    fixtures
        .seed_fleet(&fleet, &workspace, &tenant, "money-gates")
        .await;
    fixtures.seed_wallet(&tenant, FUNDED_NANOS).await;
    let event = mint_id();

    let decided = money_gates(
        &accounts,
        &biller,
        Ok(Some(tenant.clone())),
        request(&workspace, &fleet, &event),
        fixtures.enrolled_at(),
    )
    .await
    .expect("a started run is not a fault");

    let StartCheck::Start(started) = decided else {
        panic!("a funded fleet must be started, got {decided:?}");
    };
    assert_eq!(
        started.tenant_id, tenant,
        "the run is billed to the tenant the workspace resolves to"
    );
    assert_eq!(&*started.model, MODEL, "and against the model it will run");

    fixtures.cleanup().await;
}

/// The start check moves no money: the gates read, and the run's usage is charged
/// as its lines are logged once the run runs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_started_run_is_charged_nothing_at_the_start_check() {
    let fixtures = Fixtures::create().await;
    let accounts = Accounts::new(fixtures.database.clone());
    let biller = biller(&fixtures);
    let (fleet, workspace, tenant) = unique_ids();
    fixtures
        .seed_fleet(&fleet, &workspace, &tenant, "money-gates")
        .await;
    fixtures.seed_wallet(&tenant, FUNDED_NANOS).await;
    let event = mint_id();

    let decided = money_gates(
        &accounts,
        &biller,
        Ok(Some(tenant.clone())),
        request(&workspace, &fleet, &event),
        fixtures.enrolled_at(),
    )
    .await
    .expect("a started run is not a fault");

    assert!(
        matches!(decided, StartCheck::Start(_)),
        "a funded run is started: {decided:?}"
    );
    assert_eq!(
        usage_lines(&fixtures, &fleet).await,
        0,
        "the start check writes no usage line; a run is charged for what it uses"
    );

    fixtures.cleanup().await;
}

/// The money pass's verdict for one run of `fleet` on `model`.
async fn decide(
    fixtures: &Fixtures,
    (workspace, fleet, tenant): (&Uuid7, &Uuid7, &Uuid7),
    model: &str,
) -> StartCheck {
    let event = mint_id();
    money_gates(
        &Accounts::new(fixtures.database.clone()),
        &biller(fixtures),
        Ok(Some(tenant.clone())),
        priced_request(workspace, fleet, &event, model),
        fixtures.enrolled_at(),
    )
    .await
    .expect("a refusal is a decision, not a fault")
}

/// Whether `decided` refused the run for want of money.
fn refused_for_balance(decided: &StartCheck) -> bool {
    matches!(
        decided,
        StartCheck::Refuse(refusal) if refusal.label == af_core::turn::label::BALANCE_EXHAUSTED
    )
}

// Value: protects=an empty wallet refuses a priced run before anything is
// charged, and the fleet stays active; fails_when=the balance gate admits a
// floor the balance cannot cover; seam=`money_gates` over live Postgres.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_run_with_no_balance_is_refused() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, tenant) = unique_ids();
    fixtures
        .seed_fleet(&fleet, &workspace, &tenant, "money-gates")
        .await;
    fixtures.seed_model_rate(PROVIDER, PRICED_MODEL).await;
    fixtures.seed_wallet(&tenant, 0).await;

    let decided = decide(&fixtures, (&workspace, &fleet, &tenant), PRICED_MODEL).await;

    assert!(refused_for_balance(&decided), "{decided:?}");
    assert_eq!(
        usage_lines(&fixtures, &fleet).await,
        0,
        "a refused run is not charged"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM fleet.fleets WHERE id = $1")
        .bind(&fleet)
        .fetch_one(
            &mut *fixtures
                .database
                .acquire()
                .await
                .expect("a pooled connection"),
        )
        .await
        .expect("the fleet row reads");
    assert_eq!(
        status,
        afd_state::sql::FLEET_STATUS_ACTIVE,
        "the fleet stays active"
    );

    fixtures.cleanup().await;
}

// Value: protects=a payer with no wallet row is refused as an exhausted one,
// where it was once admitted; fails_when=a missing wallet reads as an
// unlimited one; seam=`money_gates` over live Postgres, the same run started
// once the wallet exists.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_payer_with_no_wallet_row_is_refused() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, tenant) = unique_ids();
    fixtures
        .seed_fleet(&fleet, &workspace, &tenant, "money-gates")
        .await;
    sqlx::query("DELETE FROM billing.wallets WHERE tenant_id = $1")
        .bind(&tenant)
        .execute(
            &mut *fixtures
                .database
                .acquire()
                .await
                .expect("a pooled connection"),
        )
        .await
        .expect("the wallet row is removed");

    let unwalleted = decide(&fixtures, (&workspace, &fleet, &tenant), MODEL).await;
    fixtures.seed_wallet(&tenant, FUNDED_NANOS).await;
    let walleted = decide(&fixtures, (&workspace, &fleet, &tenant), MODEL).await;

    assert!(refused_for_balance(&unwalleted), "{unwalleted:?}");
    assert!(matches!(walleted, StartCheck::Start(_)), "{walleted:?}");

    fixtures.cleanup().await;
}

/// How many ledger rows this fleet holds.
async fn usage_lines(fixtures: &Fixtures, fleet: &Uuid7) -> i64 {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query_scalar("SELECT COUNT(*) FROM billing.usage_lines WHERE fleet_id = $1")
        .bind(fleet)
        .fetch_one(&mut *connection)
        .await
        .expect("the usage log answers a count")
}
