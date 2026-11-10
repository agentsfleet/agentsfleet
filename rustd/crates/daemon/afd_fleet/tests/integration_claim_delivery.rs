//! The claim opens the delivery (M245 §4): one statement claims the slot,
//! reads the fleet, opens the `fleet.events` row, stamps `delivered_at` and
//! answers the fleet's counters, so the claim path costs the scan and the
//! claim, and nothing after it reads the delivery again.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use af_core::event::status;
use af_core::id::Uuid7;
use afd_db::config::DbRole;
use afd_db::test_util::statements::{self, Ran, Window};
use afd_fleet::lease::{Acquired, Delivery, Leases};

use crate::rows::{LAPSE, Row};
use crate::seed::{Seeded, seeded, select_fleet};
use crate::support::Fixtures;

/// The admission's stamped fence and delivery, its slot's token, and how
/// many event rows it has.
const DELIVERY: &str = "SELECT a.lease_fence, a.delivered_at IS NOT NULL, s.fencing_token, \
    (SELECT count(*) FROM fleet.events e WHERE e.event_id = a.id) \
    FROM fleet.admissions a LEFT JOIN runner.fleet_slots s ON s.fleet_id = a.fleet_id \
    WHERE a.id = $1";

/// What [`DELIVERY`] answers.
type DeliveryState = (Option<i64>, bool, Option<i64>, i64);

/// A private database with the statement counter installed, and its owner.
async fn counted() -> (Fixtures, afd_db::Db) {
    let fixtures = Fixtures::create_isolated().await;
    let owner = fixtures.open(DbRole::Migrator).await;
    statements::install(&owner).await;
    (fixtures, owner)
}

/// The runner's claim on `fleet`, and every statement it sent.
async fn claim_counted(
    owner: &afd_db::Db,
    leases: &Leases,
    runner: &Uuid7,
    fleet: &Uuid7,
) -> (Option<Acquired>, Ran) {
    let window = Window::open(owner).await;
    let acquired = select_fleet(leases, runner, fleet).await;
    (acquired, window.close().await)
}

async fn delivery(fixtures: &Fixtures, event: &Uuid7) -> DeliveryState {
    sqlx::query_as(DELIVERY)
        .bind(event)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("delivery state")
}

// Value: protects=the claim path's statement count; fails_when=a delivery
// write or setup read leaves the claim statement; why_new=no test counted the
// claim path; seam=none; no_claim=the scan's own plan.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn claim_opens_event_and_stamps_delivery() {
    let (fixtures, owner) = counted().await;
    let Seeded {
        runners: [runner],
        fleet,
        event_id,
        tenant,
    } = seeded::<1>(&fixtures).await;
    let leases = fixtures.leases_with_dead_bus();

    let (acquired, ran) = claim_counted(&owner, &leases, &runner, &fleet).await;
    let acquired = acquired.expect("the seeded fleet is claimed");

    assert_eq!(acquired.event_id, event_id);
    assert_eq!(acquired.opened.received.delivery, Delivery::First);
    assert_eq!(
        acquired.opened.tenant_id, tenant,
        "the payer rides the claim"
    );
    let (fence, delivered, token, rows) = delivery(&fixtures, &event_id).await;
    assert!(delivered, "delivered_at is stamped");
    assert_eq!(rows, 1, "the event row is open");
    assert_eq!(fence, token, "the admission carries the slot's fence");
    let status: String = sqlx::query_scalar("SELECT status FROM fleet.events WHERE event_id = $1")
        .bind(&event_id)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("event status");
    assert_eq!(status, status::RECEIVED);
    assert_eq!(
        ran.total(),
        2,
        "the scan and the claim: {:?}",
        ran.statements()
    );
    drop(leases);
    owner.close().await;
    fixtures.cleanup().await;
}

// Value: protects=a paused fleet runs nothing; fails_when=the claim moves a
// fence or opens a row for a fleet paused after the scan; why_new=the claim
// now reads the fleet itself; seam=none; no_claim=a pause mid-run.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn claim_on_paused_fleet_takes_nothing() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [runner],
        fleet,
        event_id,
        ..
    } = seeded::<1>(&fixtures).await;
    let before = delivery(&fixtures, &event_id).await;
    // The pause lands after a scan found the fleet: the claim statement is
    // run alone, with the binds the claim path sends.
    sqlx::query("UPDATE fleet.fleets SET status = 'paused' WHERE id = $1")
        .bind(&fleet)
        .execute(&mut *fixtures.database.acquire().await.expect("operator"))
        .await
        .expect("the operator pauses the fleet");
    let claimed = sqlx::query(afd_fleet::lease::sql::queue::CLAIM_HEAD)
        .bind(&fleet)
        .bind(&runner)
        .bind(af_core::timing::LEASE_TTL)
        .bind(af_core::timing::RUNNER_OFFLINE_AFTER)
        .bind(afd_fleet::lease::sql::RUNNER_STATUS_ACTIVE)
        .bind(afd_fleet::lease::sql::LEASE_STATUS_ACTIVE)
        .bind(afd_fleet::lease::sql::LEASE_STATUS_EXPIRED)
        .bind(af_wire::task::CAPABILITY)
        .bind(af_wire::task::TaskState::Ready.as_str())
        .bind(af_wire::task::TaskState::Running.as_str())
        .bind(afd_fleet::task::DECISION_POLICY_PATH)
        .bind(afd_fleet::lease::sql::FLEET_STATUS_ACTIVE)
        .bind(status::RECEIVED)
        .bind(afd_admission::Producer::Steer.as_str())
        .fetch_optional(&mut *fixtures.database.acquire().await.expect("claimer"))
        .await
        .expect("the claim statement runs");

    assert!(claimed.is_none(), "a paused fleet claims nothing");
    assert_eq!(
        delivery(&fixtures, &event_id).await,
        before,
        "no fence moved, nothing was stamped and no row opened"
    );
    fixtures.cleanup().await;
}

// Value: protects=redelivery classified in the claim; fails_when=a repeat or a
// finished event needs a status read after the claim; why_new=the classification
// moved into the insert's conflict arm; seam=none; no_claim=a reclaim's billing.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn claim_redelivery_reports_repeat_in_one_statement() {
    let (fixtures, owner) = counted().await;
    let Seeded {
        runners: [runner],
        fleet,
        event_id,
        ..
    } = seeded::<1>(&fixtures).await;
    let leases = fixtures.leases_with_dead_bus();
    let first = select_fleet(&leases, &runner, &fleet)
        .await
        .expect("the first claim");
    assert_eq!(first.opened.received.delivery, Delivery::First);

    // The holder stops renewing: a lapsed lease's admission is claimed again.
    fixtures
        .age(Row::Slot(&fleet), "lease_expires_at", LAPSE)
        .await;
    let (repeat, ran) = claim_counted(&owner, &leases, &runner, &fleet).await;
    let repeat = repeat.expect("the lapsed admission is claimed again");
    assert_eq!(repeat.event_id, event_id);
    assert_eq!(repeat.opened.received.delivery, Delivery::Repeat);
    assert_eq!(
        repeat.opened.received.counters, None,
        "a repeat carries no counters"
    );
    assert!(repeat.redelivered);
    assert_eq!(ran.total(), 2, "no third statement: {:?}", ran.statements());

    // The event row finished while its admission was still unsettled.
    sqlx::query("UPDATE fleet.events SET status = $2 WHERE event_id = $1")
        .bind(&event_id)
        .bind(status::PROCESSED)
        .execute(&mut *fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("the event row finishes");
    fixtures
        .age(Row::Slot(&fleet), "lease_expires_at", LAPSE)
        .await;
    let (terminal, ran) = claim_counted(&owner, &leases, &runner, &fleet).await;
    let terminal = terminal.expect("the finished admission is claimed again");
    assert_eq!(terminal.opened.received.delivery, Delivery::Terminal);
    assert_eq!(ran.total(), 2, "no third statement: {:?}", ran.statements());
    drop(leases);
    owner.close().await;
    fixtures.cleanup().await;
}

// Value: protects=the opening frame's counters; fails_when=the claim answers the
// counter row as its own statement saw it, missing its own insert;
// why_new=the counters moved into the statement that inserts; seam=none;
// no_claim=a concurrent insert, which the next frame repairs.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn claim_counters_include_its_own_event() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [runner],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;
    // Three finished events; the counting trigger moves the counter to 3.
    sqlx::query(
        "INSERT INTO fleet.events (fleet_id, workspace_id, event_id, actor, event_type, status, \
         request_json, created_at, updated_at) \
         SELECT f.id, f.workspace_id, uuidv7(), 'fixture:steer', 'chat', $2, '{}', now(), now() \
         FROM fleet.fleets f, generate_series(1, 3) WHERE f.id = $1",
    )
    .bind(&fleet)
    .bind(status::PROCESSED)
    .execute(&mut *fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("three earlier events");
    let leases = fixtures.leases_with_dead_bus();

    let claimed = select_fleet(&leases, &runner, &fleet)
        .await
        .expect("the seeded fleet is claimed");
    let counters = claimed
        .opened
        .received
        .counters
        .expect("a first delivery carries counters");
    assert_eq!(
        counters.events_processed, 4,
        "the snapshot plus its own row"
    );
    assert_eq!(
        afd_events::fleet_counters(&fixtures.database, &fleet)
            .await
            .expect("committed counters")
            .events_processed,
        4
    );
    fixtures.cleanup().await;
}
