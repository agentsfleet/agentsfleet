//! Transaction start order must not move new work ahead of an unfinished run.

use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_core::id::Uuid7;
use afd_fleet::lease::{Acquired, Leases};
use sqlx::Connection as _;

use super::{next, reclaim::issue};
use crate::{rows::Row, seed::seeded_parts, support::Fixtures};

const ORDER: &str = "SELECT claimed.sequence < delayed.sequence \
    AND delayed.created_at < claimed.created_at \
    FROM fleet.admissions claimed, fleet.admissions delayed \
    WHERE claimed.id=$1::uuid AND delayed.id=$2::uuid";

// Value: protects=unfinished work survives an older transaction committing later;
// fails_when=created_at outranks serialized insertion order; why_new=real producer
// transactions invert timestamp order; seam=Postgres plus admission and lease stores.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_late_committing_admission_does_not_displace_the_claimed_head() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, tenant, [holder, poller]) = seeded_parts::<2>(&fixtures).await;
    let admissions = Admissions::for_tests(fixtures.database.clone(), crate::queue::unreachable());
    let delayed = admissions
        .prepare(input(&fleet, &workspace))
        .await
        .expect("prepare");
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("older transaction");
    let mut tx = connection.begin().await.expect("start before the winner");
    // Separate the transactions' millisecond timestamps before the second starts.
    sqlx::query("SELECT pg_sleep(0.002)")
        .execute(&mut *tx)
        .await
        .expect("clock tick");
    let claimed = admissions
        .admit(input(&fleet, &workspace))
        .await
        .expect("newer commits first");
    let leases = fixtures.leases_with_dead_queue();
    let first = next(&leases, &holder).await.expect("first claim");
    assert_eq!(first.event_id, claimed.stored.id);
    leases.record_received(&first).await.expect("delivery");
    let issued = issue(&fixtures, &leases, &first, &holder, &tenant).await;
    let delayed = admissions
        .stage(&mut tx, delayed)
        .await
        .expect("older inserts second");
    tx.commit().await.expect("late commit");
    let inverted: bool = sqlx::query_scalar(ORDER)
        .bind(&claimed.stored.id)
        .bind(&delayed.stored.id)
        .fetch_one(&mut *connection)
        .await
        .expect("actual transaction order");
    assert!(inverted, "the old query would select the new admission");
    fixtures.lapse_holder(&issued, &fleet).await;
    assert_reclaimed(&fixtures, &leases, &poller, &first, &issued, &tenant).await;
    drop((connection, leases, admissions));
    fixtures.cleanup().await;
}

async fn assert_reclaimed(
    fixtures: &Fixtures,
    leases: &Leases,
    poller: &Uuid7,
    first: &Acquired,
    issued: &str,
    tenant: &str,
) {
    let recovered = next(leases, poller).await.expect("recover unfinished run");
    assert_eq!(recovered.event_id, first.event_id);
    assert!(recovered.redelivered);
    assert!(recovered.fence > first.fence);
    assert_eq!(
        recovered.reused.expect("original billing").tenant_id,
        tenant
    );
    assert_eq!(
        fixtures.text(Row::Lease(issued), "status").await.as_deref(),
        Some(afd_fleet::lease::sql::LEASE_STATUS_EXPIRED)
    );
}

fn input<'a>(fleet: &'a str, workspace: &'a str) -> Admission<'a> {
    Admission {
        producer: Producer::Steer,
        key: Key::Unrepeatable,
        fleet,
        workspace,
        actor: crate::seed::ACTOR,
        event_type: afd_wire::event::EventType::Chat,
        request_json: crate::seed::REQUEST_JSON,
        reply: Reply::None,
    }
}

/// How long a send may take to start waiting on the held fleet lock.
const BLOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Sessions waiting on `holder`, read from a separate autocommit connection (SQL-STATACT).
const BLOCKED_ON: &str =
    "SELECT count(*) FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid))";

/// A send that waits behind a staged admission's fleet lock draws the later
/// sequence, and the claim takes the staged row first (`M242_001` Dimension 3.1).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_send_queued_behind_the_fleet_lock_takes_the_later_sequence() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, _tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let admissions = Admissions::for_tests(fixtures.database.clone(), crate::queue::unreachable());
    let prepared = admissions
        .prepare(input(&fleet, &workspace))
        .await
        .expect("prepare");
    let mut connection = fixtures.database.acquire().await.expect("holder");
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await
        .expect("holder backend");
    let mut tx = connection
        .begin()
        .await
        .expect("open the staging transaction");
    let staged = admissions
        .stage(&mut tx, prepared)
        .await
        .expect("stage under the open transaction");
    let queued_send = tokio::spawn({
        let (admissions, fleet, workspace) = (admissions.clone(), fleet.clone(), workspace.clone());
        async move { admissions.admit(input(&fleet, &workspace)).await }
    });
    let deadline = tokio::time::Instant::now() + BLOCK_WAIT;
    loop {
        let blocked: i64 = sqlx::query_scalar(BLOCKED_ON)
            .bind(holder)
            .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
            .await
            .expect("blocked sessions");
        if blocked > 0 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the send never waited on the fleet lock"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    tx.commit().await.expect("the staged admission commits");
    let behind = queued_send
        .await
        .expect("the waiting send's task")
        .expect("the waiting send admits");
    let later: bool = sqlx::query_scalar(
        "SELECT w.sequence > s.sequence FROM fleet.admissions w, fleet.admissions s \
         WHERE w.id = $1::uuid AND s.id = $2::uuid",
    )
    .bind(&behind.stored.id)
    .bind(&staged.stored.id)
    .fetch_one(&mut *connection)
    .await
    .expect("both sequences");
    assert!(later, "the waiter draws the later sequence");
    let leases = fixtures.leases_with_dead_queue();
    let first = next(&leases, &runner).await.expect("the claim");
    assert_eq!(
        first.event_id, staged.stored.id,
        "the staged row is claimed first"
    );
}
