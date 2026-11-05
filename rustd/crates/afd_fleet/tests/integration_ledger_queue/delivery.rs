//! The claim's writes land or fail together, and a claim never waits on a
//! producer.

use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_db::config::DbRole;
use afd_fleet::lease::{Delivery, Leases};
use afd_wire::event::EventType;
use sqlx::Connection as _;

use super::{WITHIN, enqueue, next};
use crate::{seed::seeded_parts, support::Fixtures};

const FAIL_DELIVERY: &str = "CREATE FUNCTION fleet.refuse_delivery() RETURNS trigger \
    LANGUAGE plpgsql AS $$ BEGIN \
      RAISE EXCEPTION 'injected delivery failure'; END $$; \
    CREATE TRIGGER refuse_delivery BEFORE UPDATE OF delivered_at ON fleet.admissions \
    FOR EACH ROW EXECUTE FUNCTION fleet.refuse_delivery()";
const RESTORE_DELIVERY: &str = "DROP TRIGGER refuse_delivery ON fleet.admissions; \
    DROP FUNCTION fleet.refuse_delivery()";
const SLOT_FENCE: &str = "SELECT fencing_token FROM runner.fleet_slots WHERE fleet_id = $1::uuid";

// Value: protects=atomic claim; fails_when=a refused stamp leaves the fence,
// the narrative or the counters moved; why_new=success misses SQL faults;
// seam=none; no_claim=every failure ordinal.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_failed_delivery_stamp_writes_nothing_then_the_claim_lands_once() {
    let fixtures = Fixtures::create_isolated_with_queue().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_queue();
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    let before = afd_events::fleet_counters(&fixtures.database, &fleet)
        .await
        .expect("counters");
    let fence_before = slot_fence(&fixtures, &fleet).await;

    sqlx::raw_sql(FAIL_DELIVERY)
        .execute(&mut *connection)
        .await
        .expect("inject fault");
    let refused = leases
        .select(&runner, &[])
        .await
        .expect_err("a refused stamp fails the whole claim");
    assert!(refused.to_string().contains("ledger claim"), "{refused}");
    assert_eq!(delivery_state(&fixtures, &event).await, (0, false));
    assert_eq!(slot_fence(&fixtures, &fleet).await, fence_before);
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

    let first = next(&leases, &runner).await.expect("claim");
    assert_eq!(first.delivery, Delivery::First);
    assert_eq!(delivery_state(&fixtures, &event).await, (1, true));
    let counters = afd_events::fleet_counters(&fixtures.database, &fleet)
        .await
        .expect("committed counters");
    assert_eq!(counters.events_processed, before.events_processed + 1);

    leases
        .release(&first.fleet_id, first.fence)
        .await
        .expect("the claim is freed");
    let again = next(&leases, &runner).await.expect("the same admission");
    assert_eq!(
        (again.event_id.as_str(), again.delivery),
        (event.as_str(), Delivery::Repeat)
    );
    assert_eq!(delivery_state(&fixtures, &event).await, (1, true));
    assert_eq!(
        afd_events::fleet_counters(&fixtures.database, &fleet)
            .await
            .expect("repeat counters"),
        counters,
        "a redelivery opens no second row and moves no count"
    );
    drop((connection, leases));
    owner.close().await;
    fixtures.cleanup().await;
}

// Value: protects=the received frame's counter read is optional; fails_when=a
// refused read faults the pass; why_new=success misses grant faults;
// seam=none; no_claim=every read fault.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_refused_counter_read_leaves_the_delivery_standing() {
    let fixtures = Fixtures::create_isolated_with_queue().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_queue();
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    sqlx::raw_sql("REVOKE SELECT ON fleet.activity_counters FROM api_runtime")
        .execute(&mut *connection)
        .await
        .expect("refuse only the optional read; the trigger uses its owner's grant");
    let acquired = next(&leases, &runner).await.expect("claim");
    leases.publish_opened(&acquired).await;
    let state = delivery_state(&fixtures, &event).await;
    sqlx::raw_sql("GRANT SELECT ON fleet.activity_counters TO api_runtime")
        .execute(&mut *connection)
        .await
        .expect("restore read grant before checking durability");
    assert_eq!(
        (acquired.delivery, state),
        (Delivery::First, (1, true)),
        "a refused counter read cannot undo the delivery"
    );
    drop((connection, leases));
    owner.close().await;
    fixtures.cleanup().await;
}

// Value: protects=claim/producer lock compatibility; fails_when=a claim waits
// on a producer's uncommitted admission; why_new=serial delivery misses
// producer locks; seam=none; no_claim=every possible deadlock cycle.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_claim_skips_a_fleet_a_producer_is_admitting_to_and_never_waits() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_queue();
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
        .expect("producer holds the fleet and its slot");
    // gstack-timing-ok: deadlock guard; the producer stays uncommitted while the claim runs.
    let skipped = tokio::time::timeout(WITHIN, claim(&leases, &runner))
        .await
        .expect("the claim does not wait on the producer");
    assert!(skipped.is_none(), "the slot the producer holds is skipped");
    pending.rollback().await.expect("discard pending producer");
    let acquired = next(&leases, &runner)
        .await
        .expect("the fleet is free again");
    assert_eq!(
        (acquired.event_id, acquired.delivery),
        (event, Delivery::First)
    );
    drop((connection, leases, admissions));
    fixtures.cleanup().await;
}

async fn claim(
    leases: &Leases,
    runner: &afd_core::id::Uuid7,
) -> Option<afd_fleet::lease::Acquired> {
    leases.select(runner, &[]).await.expect("valid claim")
}

async fn slot_fence(fixtures: &Fixtures, fleet: &str) -> Option<i64> {
    sqlx::query_scalar(SLOT_FENCE)
        .bind(fleet)
        .fetch_optional(&mut *fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("slot fence")
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
