//! Event deadlines allow recovery; standing access still needs a human answer.

use afd_admission::Admissions;
use afd_approval::{Decision, Inbox, Resolution};
use afd_core::id::Uuid7;
use afd_fleet::lease::sql::{self, queue::CLAIM_HEAD};
use sqlx::Connection as _;

use super::{enqueue, next, parking::seed_gate};
use crate::{seed::seeded_parts, support::Fixtures};

const SCOPE: &str = "UPDATE fleet.approval_gates SET event_id=$2 WHERE action_id=$1";
const EXPIRE: &str = "UPDATE fleet.approval_gates \
    SET expires_at=clock_timestamp()-interval '1 second' WHERE action_id=$1";
const PENDING: &str = "SELECT count(*) FROM fleet.approval_gates \
    WHERE fleet_id=$1::uuid AND resolved_at IS NULL";

// Value: protects=deadline recovery and the last live matching blocker;
// fails_when=any expired gate admits a head still held by a live gate, or expired
// gates never requalify it; why_new=no sweeper runs; seam=real gate and queue rows.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn parked_heads_wait_for_all_matching_deadlines_without_expiring_the_cards() {
    for first_gate_first in [false, true] {
        deadline_case(first_gate_first).await;
    }
}

async fn deadline_case(first_gate_first: bool) {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let head = enqueue(&fixtures, &fleet, &workspace).await;
    let later = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_queue();
    let first = next(&leases, &runner).await.expect("initial head");
    let other = scoped_gate(&fixtures, &fleet, &workspace, &head).await;
    let specific = scoped_gate(&fixtures, &fleet, &workspace, &head).await;
    scoped_gate(&fixtures, &fleet, &workspace, &later).await;
    leases
        .park(&first.fleet_id, &head, first.fence)
        .await
        .expect("park on approvals");
    assert!(
        next(&leases, &runner).await.is_none(),
        "live gates block the head and tail"
    );
    assert_blocked_claim(&fixtures, &fleet, &runner).await;
    let (early, last) = if first_gate_first {
        (&other, &specific)
    } else {
        (&specific, &other)
    };
    lapse(&fixtures, early).await;
    assert!(
        next(&leases, &runner).await.is_none(),
        "one matching gate is still live"
    );
    assert_blocked_claim(&fixtures, &fleet, &runner).await;
    lapse(&fixtures, last).await;
    let resumed = next(&leases, &runner)
        .await
        .expect("all matching gates lapsed");
    assert_eq!(
        resumed.event_id, head,
        "the later event's gate is unrelated"
    );
    assert!(resumed.fence > first.fence);
    assert_answerable(&fixtures, &fleet, &specific).await;
    drop(leases);
    fixtures.cleanup().await;
}

async fn assert_answerable(fixtures: &Fixtures, fleet: &str, specific: &str) {
    let count: i64 = sqlx::query_scalar(PENDING)
        .bind(fleet)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("pending cards");
    assert_eq!(count, 3, "discovery and claim do not expire approval cards");
    approve(fixtures, specific).await;
}

async fn approve(fixtures: &Fixtures, action: &str) {
    let queue = crate::queue::unreachable();
    let inbox = Inbox::new(
        fixtures.database.clone(),
        queue.clone(),
        Admissions::for_tests(fixtures.database.clone(), queue),
    );
    assert!(matches!(
        inbox
            .resolve(action, Decision::Approved, "fixture:human", "", None)
            .await
            .expect("late answer"),
        Resolution::Resolved(_)
    ));
    drop(inbox);
}

// Value: protects=standing access cannot loop through claim and park after its
// deadline; fails_when=deadline alone wakes it; seam=real approval and slot rows.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn standing_access_waits_for_an_answer_after_its_card_deadline() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let head = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_queue();
    let first = next(&leases, &runner).await.expect("initial head");
    let action = seed_gate(&fixtures, &fleet, &workspace).await;
    leases
        .park(&first.fleet_id, &head, first.fence)
        .await
        .expect("park on access");
    lapse(&fixtures, &action).await;
    assert!(
        next(&leases, &runner).await.is_none(),
        "only an answer resolves access"
    );
    assert_blocked_claim(&fixtures, &fleet, &runner).await;
    approve(&fixtures, &action).await;
    assert_eq!(
        next(&leases, &runner)
            .await
            .expect("answered access")
            .event_id,
        head
    );
    drop(leases);
    fixtures.cleanup().await;
}

async fn scoped_gate(fixtures: &Fixtures, fleet: &str, workspace: &str, event: &str) -> String {
    let action = seed_gate(fixtures, fleet, workspace).await;
    sqlx::query(SCOPE)
        .bind(&action)
        .bind(event)
        .execute(&mut *fixtures.database.acquire().await.expect("gate scope"))
        .await
        .expect("event-specific gate");
    action
}

async fn lapse(fixtures: &Fixtures, action: &str) {
    sqlx::query(EXPIRE)
        .bind(action)
        .execute(&mut *fixtures.database.acquire().await.expect("gate clock"))
        .await
        .expect("deadline in the past");
}

async fn assert_blocked_claim(fixtures: &Fixtures, fleet: &str, runner: &Uuid7) {
    let mut connection = fixtures.database.acquire().await.expect("claim connection");
    let before: i64 =
        sqlx::query_scalar("SELECT fencing_token FROM runner.fleet_slots WHERE fleet_id=$1::uuid")
            .bind(fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("slot before refusal");
    let mut tx = connection.begin().await.expect("direct claim transaction");
    let row = sqlx::query(CLAIM_HEAD)
        .bind(fleet)
        .bind(runner.as_str())
        .bind(afd_core::timing::LEASE_TTL)
        .bind(afd_core::timing::RUNNER_OFFLINE_AFTER)
        .bind(sql::RUNNER_STATUS_ACTIVE)
        .bind(sql::LEASE_STATUS_ACTIVE)
        .bind(sql::LEASE_STATUS_EXPIRED)
        .fetch_optional(&mut *tx)
        .await
        .expect("claim checks the same gate rule");
    assert!(row.is_none());
    tx.rollback()
        .await
        .expect("empty claim rolls back the slot");
    let after: i64 =
        sqlx::query_scalar("SELECT fencing_token FROM runner.fleet_slots WHERE fleet_id=$1::uuid")
            .bind(fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("slot after refusal");
    assert_eq!(after, before);
}
