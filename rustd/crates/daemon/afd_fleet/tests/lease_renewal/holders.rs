//! What must hold after any ordering: one live holder, one full host.

use super::{
    expiry::snapshot,
    fixture::Running,
    replica::{Replica, original},
};
use afd_core::{clock, id::Uuid7};
use afd_fleet::lease::Renewed;
use jiff::Timestamp;
use sqlx::Connection as _;

const LIVE_HOLDERS: &str = "\
SELECT id::text FROM runner.leases
WHERE fleet_id = $1::uuid AND status = $2 AND lease_expires_at > clock_timestamp()";
const EVENT_LEASES: &str = "\
SELECT count(*) FROM runner.leases l JOIN fleet.events e ON e.id = l.fleet_event_id
WHERE l.fleet_id = $1::uuid AND e.event_id = $2";
const HOST_OCCUPANCY: &str = "\
SELECT r.worker_count::bigint,
  (SELECT count(*) FROM runner.leases l WHERE l.runner_id = r.id AND l.status = $2
     AND l.lease_expires_at > clock_timestamp())
  + (SELECT count(*) FROM authoring.attempts a WHERE a.runner_id = r.id
     AND a.status = $2 AND a.lease_expires_at > clock_timestamp())
FROM runner.runners r WHERE r.id = $1::uuid";

/// The state both replicas left, read back after the race.
pub(super) struct Outcome<'a> {
    pub(super) order: &'a str,
    pub(super) original_event: &'a str,
    pub(super) readmitted: &'a Uuid7,
    pub(super) expired: (Timestamp, Timestamp, Timestamp),
}

/// B alone holds the work, A's row is retired untouched, and the host is full.
pub(super) async fn assert_one_holder(running: &Running, replica: &Replica, outcome: &Outcome<'_>) {
    let order = outcome.order;
    let after = snapshot(running).await;
    assert_eq!(
        (after.0, after.1),
        (outcome.expired.0, outcome.expired.1),
        "{order}: the refused renewal moved neither the old deadline nor its meter"
    );
    assert_eq!(
        original(running).await,
        (
            outcome.original_event.to_owned(),
            afd_runner::sql::LEASE_STATUS_EXPIRED.to_owned()
        ),
        "{order}: re-admission retires the original holder's row"
    );
    assert_eq!(
        live_holders(running).await,
        vec![outcome.readmitted.to_string()],
        "{order}: exactly one live holder, and it is the re-admitting replica"
    );
    assert_eq!(
        event_leases(running, outcome.original_event).await,
        2,
        "{order}: one expired and one re-admitted lease; no third admission"
    );
    assert_host_full(running, order).await;
    let pricing = afd_fleet::lease::usage::Pricing::load(
        &running.fixtures.database,
        afd_billing::Posture::SelfManaged,
        crate::seed::PROVIDER,
        crate::seed::MODEL,
        outcome.readmitted.as_str(),
    )
    .await;
    let renewed = replica
        .leases
        .extend(
            afd_fleet::lease::sql::meter::Metering {
                lease_id: outcome.readmitted.as_str(),
                runner_id: &running.runner,
                requests: &[],
            },
            outcome.readmitted,
            &pricing,
            clock::now(),
        )
        .await
        .expect("the winner's renewal is a verdict");
    assert!(
        matches!(renewed, Renewed::Extended { .. }),
        "{order}: the winning holder keeps renewing, got {renewed:?}"
    );
}

async fn live_holders(running: &Running) -> Vec<String> {
    sqlx::query_scalar(LIVE_HOLDERS)
        .bind(&running.fleet)
        .bind(afd_runner::sql::LEASE_STATUS_ACTIVE)
        .fetch_all(&mut *running.fixtures.database.acquire().await.expect("read"))
        .await
        .expect("live holders")
}

async fn event_leases(running: &Running, event: &str) -> i64 {
    sqlx::query_scalar(EVENT_LEASES)
        .bind(&running.fleet)
        .bind(event)
        .fetch_one(&mut *running.fixtures.database.acquire().await.expect("read"))
        .await
        .expect("leases for the event")
}

/// The census both admission paths lock before admitting — fleet issue and
/// authoring claim alike — reports a full host that refuses another lease.
pub(super) async fn assert_host_full(running: &Running, order: &str) {
    let mut connection = running.fixtures.database.acquire().await.expect("census");
    let (workers, occupied): (i64, i64) = sqlx::query_as(HOST_OCCUPANCY)
        .bind(running.runner.as_str())
        .bind(afd_runner::sql::LEASE_STATUS_ACTIVE)
        .fetch_one(&mut *connection)
        .await
        .expect("host occupancy");
    assert_eq!(
        (workers, occupied),
        (1, 1),
        "{order}: one worker, one live lease"
    );
    let mut tx = connection.begin().await.expect("census transaction");
    let capacity = afd_runner::capacity::lock(&mut tx, &running.runner)
        .await
        .expect("lock host")
        .expect("host exists");
    assert!(!capacity.can_claim(), "{order}: no further admission fits");
    tx.rollback().await.expect("release census");
}
