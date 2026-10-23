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
