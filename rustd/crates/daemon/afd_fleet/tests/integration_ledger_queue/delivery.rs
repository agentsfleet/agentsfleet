//! The claim's delivery writes roll back together and coexist with producer
//! budget locks.
//!
//! One statement claims the slot, opens the `fleet.events` row and stamps
//! `delivered_at` (M245 §4), so a fault in any of them leaves none behind.

use af_core::id::Uuid7;
use af_wire::event::EventType;
use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_db::config::DbRole;
use afd_fleet::lease::Delivery;
use sqlx::Connection as _;

use super::{STAMPS, WITHIN, enqueue, next};
use crate::{seed::seeded_parts, support::Fixtures};

const FAIL_DELIVERY: &str = "CREATE FUNCTION fleet.refuse_delivery() RETURNS trigger \
    LANGUAGE plpgsql AS $$ BEGIN \
      RAISE EXCEPTION 'injected delivery failure'; END $$; \
    CREATE TRIGGER refuse_delivery BEFORE UPDATE OF delivered_at ON fleet.admissions \
    FOR EACH ROW EXECUTE FUNCTION fleet.refuse_delivery()";
const RESTORE_DELIVERY: &str = "DROP TRIGGER refuse_delivery ON fleet.admissions; \
    DROP FUNCTION fleet.refuse_delivery()";

// Value: protects=atomic delivery; fails_when=a stamp fault leaves a moved fence,
// an open row or counted event behind; why_new=success misses SQL faults;
// seam=none; no_claim=every failure ordinal.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_failed_delivery_stamp_rolls_back_the_whole_claim_then_claims_once() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_bus();
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    let before = afd_events::fleet_counters(&fixtures.database, &fleet)
        .await
        .expect("counters");

    sqlx::raw_sql(FAIL_DELIVERY)
        .execute(&mut *connection)
        .await
        .expect("inject fault");
    tokio::time::timeout(WITHIN, leases.select(&runner, &[]))
        .await
        .expect("the claim never waits")
        .expect_err("a refused stamp refuses the claim");
    assert_eq!(delivery_state(&fixtures, &event).await, (0, false));
    assert_eq!(
        fence(&fixtures, &event).await,
        (None, None),
        "no fence moved"
    );
    assert_eq!(
        afd_events::fleet_counters(&fixtures.database, &fleet)
            .await
            .expect("rollback counters"),
        before
    );
    sqlx::raw_sql(RESTORE_DELIVERY)
        .execute(&mut *connection)
        .await
        .expect("remove fault");

    let acquired = next(&leases, &runner).await.expect("claim");
    assert_eq!(acquired.opened.received.delivery, Delivery::First);
    assert_eq!(delivery_state(&fixtures, &event).await, (1, true));
    let after = afd_events::fleet_counters(&fixtures.database, &fleet)
        .await
        .expect("committed counters");
    assert_eq!(after.events_processed, before.events_processed + 1);
    assert_eq!(
        acquired.opened.received.counters,
        Some(after),
        "the claim answers the counters its own insert moved"
    );
    drop((connection, leases));
    owner.close().await;
    fixtures.cleanup().await;
}

// Value: protects=producer/delivery lock compatibility; fails_when=budget lock
// blocks the event foreign key; why_new=serial delivery misses producer locks;
// seam=none; no_claim=every possible deadlock cycle.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn first_delivery_finishes_while_a_producer_holds_the_fleet_budget_lock() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_bus();
    let admissions =
        Admissions::for_tests(fixtures.database.clone(), afd_bus::test_util::unreachable());
    let prepared = admissions
        .prepare(Admission {
            producer: Producer::Steer,
            key: Key::Repeated("held-producer"),
            fleet: &fleet,
            workspace: &workspace,
            actor: "fixture:producer",
            event_type: EventType::Chat,
            request_json: "{}",
            reply: Reply::None,
        })
        .await
        .expect("prepare producer");
    let mut connection = fixtures.database.acquire().await.expect("producer");
    let mut pending = connection.begin().await.expect("producer transaction");
    admissions
        .stage(&mut pending, prepared)
        .await
        .expect("producer holds budget lock");
    // `next` bounds the claim by WITHIN: the producer stays uncommitted until
    // delivery finishes, so a claim that waited on it would time out.
    let acquired = next(&leases, &runner).await.expect("claim");
    assert_eq!(acquired.event_id, event);
    assert_eq!(acquired.opened.received.delivery, Delivery::First);
    assert_eq!(delivery_state(&fixtures, &event).await, (1, true));
    pending.rollback().await.expect("discard pending producer");
    drop((connection, leases, admissions));
    fixtures.cleanup().await;
}

async fn delivery_state(fixtures: &Fixtures, event: &Uuid7) -> (i64, bool) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM fleet.events WHERE event_id=$1), \
        delivered_at IS NOT NULL FROM fleet.admissions WHERE id=$1",
    )
    .bind(event)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("delivery state")
}

/// The admission's stamped fence and its slot's token.
async fn fence(fixtures: &Fixtures, event: &Uuid7) -> (Option<i64>, Option<i64>) {
    sqlx::query_as(STAMPS)
        .bind(event)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("fence state")
}
