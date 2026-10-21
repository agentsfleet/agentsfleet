//! Delivery writes roll back together and coexist with producer budget locks.

use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_db::config::DbRole;
use afd_fleet::lease::{Acquired, Delivery, Leases, Received};
use afd_wire::event::EventType;
use sqlx::Connection as _;

use super::{WITHIN, enqueue, next};
use crate::{seed::seeded_parts, support::Fixtures};

const FAIL_DELIVERY: &str = "CREATE FUNCTION fleet.refuse_delivery() RETURNS trigger \
    LANGUAGE plpgsql AS $$ BEGIN \
      IF NOT EXISTS (SELECT 1 FROM fleet.events \
        WHERE fleet_id = NEW.fleet_id AND event_id = NEW.id::text) THEN \
        RAISE EXCEPTION 'narrative was not inserted'; \
      END IF; \
      RAISE EXCEPTION 'injected delivery failure after narrative'; END $$; \
    CREATE TRIGGER refuse_delivery BEFORE UPDATE OF delivered_at ON fleet.admissions \
    FOR EACH ROW EXECUTE FUNCTION fleet.refuse_delivery()";
const RESTORE_DELIVERY: &str = "DROP TRIGGER refuse_delivery ON fleet.admissions; \
    DROP FUNCTION fleet.refuse_delivery()";

// Value: protects=durable receive; fails_when=stamp failure leaves partial writes
// or optional counter read rolls back delivery; why_new=success misses SQL faults;
// seam=none; no_claim=every failure ordinal.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_failed_delivery_stamp_rolls_back_narrative_and_counters_then_retries_once() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_queue();
    let acquired = next(&leases, &runner).await.expect("claim");
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    assert_stamp_failure_rolls_back(&fixtures, &mut connection, &leases, &acquired).await;
    let first =
        receive_with_counter_read_refused(&fixtures, &mut connection, &leases, &acquired).await;
    assert_eq!(first.delivery, Delivery::First);
    assert_eq!(delivery_state(&fixtures, &event).await, (1, true));
    let counters = afd_events::fleet_counters(&fixtures.database, &fleet)
        .await
        .expect("committed counters");
    assert_eq!(first.counters, None);
    assert_eq!(
        leases
            .record_received(&acquired)
            .await
            .expect("repeat")
            .delivery,
        Delivery::Repeat
    );
    assert_eq!(delivery_state(&fixtures, &event).await, (1, true));
    assert_eq!(
        afd_events::fleet_counters(&fixtures.database, &fleet)
            .await
            .expect("repeat counters"),
        counters
    );
    drop((connection, leases));
    owner.close().await;
    fixtures.cleanup().await;
}

async fn assert_stamp_failure_rolls_back(
    fixtures: &Fixtures,
    connection: &mut sqlx::PgConnection,
    leases: &Leases,
    acquired: &Acquired,
) {
    let before = afd_events::fleet_counters(&fixtures.database, acquired.fleet_id.as_str())
        .await
        .expect("counters");
    sqlx::raw_sql(FAIL_DELIVERY)
        .execute(&mut *connection)
        .await
        .expect("inject fault");
    let failure = leases
        .record_received(acquired)
        .await
        .expect_err("stamp refuses delivery");
    assert!(
        failure.to_string().contains("admission delivered stamp"),
        "{failure}"
    );
    assert_eq!(
        delivery_state(fixtures, &acquired.event_id).await,
        (0, false)
    );
    assert_eq!(
        afd_events::fleet_counters(&fixtures.database, acquired.fleet_id.as_str())
            .await
            .expect("rollback counters"),
        before
    );
    sqlx::raw_sql(RESTORE_DELIVERY)
        .execute(connection)
        .await
        .expect("remove fault");
}

async fn receive_with_counter_read_refused(
    fixtures: &Fixtures,
    connection: &mut sqlx::PgConnection,
    leases: &Leases,
    acquired: &Acquired,
) -> Received {
    let before = afd_events::fleet_counters(&fixtures.database, acquired.fleet_id.as_str())
        .await
        .expect("counters before optional read fault");
    sqlx::raw_sql("REVOKE SELECT ON fleet.activity_counters FROM api_runtime")
        .execute(&mut *connection)
        .await
        .expect("refuse only the optional read; trigger uses its owner's grant");
    let received = leases
        .record_received(acquired)
        .await
        .expect("counter failure is optional");
    let state = delivery_state(fixtures, &acquired.event_id).await;
    sqlx::raw_sql("GRANT SELECT ON fleet.activity_counters TO api_runtime")
        .execute(connection)
        .await
        .expect("restore read grant before checking durability");
    assert_eq!(state, (1, true), "counter read cannot undo delivery");
    let after = afd_events::fleet_counters(&fixtures.database, acquired.fleet_id.as_str())
        .await
        .expect("durable counters");
    assert_eq!(after.events_processed, before.events_processed + 1);
    assert_eq!(after.budget_used_nanos, before.budget_used_nanos);
    received
}

// Value: protects=producer/delivery lock compatibility; fails_when=budget lock
// blocks the event foreign key; why_new=serial delivery misses producer locks;
// seam=none; no_claim=every possible deadlock cycle.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn first_delivery_finishes_while_a_producer_holds_the_fleet_budget_lock() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_queue();
    let acquired = next(&leases, &runner).await.expect("claim");
    let admissions = Admissions::for_tests(fixtures.database.clone(), crate::queue::unreachable());
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
    // gstack-timing-ok: deadlock guard; the producer stays uncommitted until delivery finishes.
    let received = tokio::time::timeout(WITHIN, leases.record_received(&acquired))
        .await
        .expect("delivery does not wait on producer")
        .expect("foreign key remains compatible");
    assert_eq!(received.delivery, Delivery::First);
    assert_eq!(delivery_state(&fixtures, &event).await, (1, true));
    pending.rollback().await.expect("discard pending producer");
    drop((connection, leases, admissions));
    fixtures.cleanup().await;
}

async fn delivery_state(fixtures: &Fixtures, event: &str) -> (i64, bool) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM fleet.events WHERE event_id=$1), \
        delivered_at IS NOT NULL FROM fleet.admissions WHERE id=$1::uuid",
    )
    .bind(event)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("delivery state")
}
