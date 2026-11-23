//! The assignment's delivery writes roll back together and coexist with producer
//! budget locks.
//!
//! One statement assigns the thread and, in one update of the turn, stamps its
//! fence and opens its event, so a fault in any of them leaves none behind.

use af_core::id::Uuid7;
use af_wire::turn::TurnKind;
use afd_db::config::DbRole;
use afd_fleet::run::Delivery;
use afd_turns::{IncomingTurn, Key, Producer, Reply, Turns};
use sqlx::Connection as _;

use super::{STAMPS, UNASSIGNED, WITHIN, enqueue, next};
use crate::{seed::seeded_parts, support::Fixtures};

const FAIL_DELIVERY: &str = "CREATE FUNCTION fleet.refuse_delivery() RETURNS trigger \
    LANGUAGE plpgsql AS $$ BEGIN \
      RAISE EXCEPTION 'injected delivery failure'; END $$; \
    CREATE TRIGGER refuse_delivery BEFORE UPDATE OF run_fence ON thread.turns \
    FOR EACH ROW EXECUTE FUNCTION fleet.refuse_delivery()";
const RESTORE_DELIVERY: &str = "DROP TRIGGER refuse_delivery ON thread.turns; \
    DROP FUNCTION fleet.refuse_delivery()";

// Value: protects=atomic delivery; fails_when=a stamp fault leaves a moved fence,
// an open row or counted event behind; why_new=success misses SQL faults;
// seam=none; no_claim=every failure ordinal.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_failed_delivery_stamp_rolls_back_the_whole_assignment_then_assigns_once() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let runs = fixtures.runs_with_dead_bus();
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    let before = afd_turns::view::fleet_counters(&fixtures.database, &fleet)
        .await
        .expect("counters");

    sqlx::raw_sql(FAIL_DELIVERY)
        .execute(&mut *connection)
        .await
        .expect("inject fault");
    tokio::time::timeout(WITHIN, runs.select(&runner, &[]))
        .await
        .expect("the assignment never waits")
        .expect_err("a refused stamp refuses the assignment");
    assert_eq!(delivery_state(&fixtures, &event).await, (0, false));
    assert_eq!(
        fence(&fixtures, &event).await,
        (None, UNASSIGNED),
        "no fence moved"
    );
    assert_eq!(
        afd_turns::view::fleet_counters(&fixtures.database, &fleet)
            .await
            .expect("rollback counters"),
        before
    );
    sqlx::raw_sql(RESTORE_DELIVERY)
        .execute(&mut *connection)
        .await
        .expect("remove fault");

    let acquired = next(&runs, &runner).await.expect("assignment");
    assert_eq!(acquired.opened.received.delivery, Delivery::First);
    assert_eq!(delivery_state(&fixtures, &event).await, (1, true));
    let after = afd_turns::view::fleet_counters(&fixtures.database, &fleet)
        .await
        .expect("committed counters");
    assert_eq!(after.events_processed, before.events_processed + 1);
    assert_eq!(
        acquired.opened.received.counters,
        Some(after),
        "the assignment answers the counters its own opening moved"
    );
    drop((connection, runs));
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
    let runs = fixtures.runs_with_dead_bus();
    let turns = Turns::for_tests(fixtures.database.clone(), afd_bus::test_util::unreachable());
    let prepared = turns
        .prepare(IncomingTurn {
            producer: Producer::Steer,
            key: Key::Repeated("held-producer"),
            fleet: &fleet,
            workspace: &workspace,
            actor: "fixture:producer",
            event_type: TurnKind::Chat,
            request_json: "{}",
            reply: Reply::None,
        })
        .await
        .expect("prepare producer");
    let mut connection = fixtures.database.acquire().await.expect("producer");
    let mut pending = connection.begin().await.expect("producer transaction");
    turns
        .stage(&mut pending, prepared)
        .await
        .expect("producer holds budget lock");
    // `next` bounds the assignment by WITHIN: the producer stays uncommitted until
    // delivery finishes, so an assignment that waited on it would time out.
    let acquired = next(&runs, &runner).await.expect("assignment");
    assert_eq!(acquired.turn_id, event);
    assert_eq!(acquired.opened.received.delivery, Delivery::First);
    assert_eq!(delivery_state(&fixtures, &event).await, (1, true));
    pending.rollback().await.expect("discard pending producer");
    drop((connection, runs, turns));
    fixtures.cleanup().await;
}

async fn delivery_state(fixtures: &Fixtures, event: &Uuid7) -> (i64, bool) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM thread.turns WHERE id=$1 AND opened_at IS NOT NULL), \
        run_fence IS NOT NULL FROM thread.turns WHERE id=$1",
    )
    .bind(event)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("delivery state")
}

/// The turn's stamped fence and its thread's token.
async fn fence(fixtures: &Fixtures, event: &Uuid7) -> (Option<i64>, Option<i64>) {
    sqlx::query_as(STAMPS)
        .bind(event)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("fence state")
}
