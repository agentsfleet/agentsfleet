//! Event deadlines allow recovery; standing access still needs a human answer.

use af_core::id::Uuid7;
use afd_approval::{Decision, Inbox, Resolution};
use afd_fleet::run::sql::queue::ASSIGN_HEAD;
use afd_turns::Turns;
use sqlx::Connection as _;

use super::{enqueue, next, parking::seed_gate};
use crate::{seed::seeded_parts, support::Fixtures};

const SCOPE: &str = "UPDATE fleet.approval_gates SET turn_id=$2 WHERE action_id=$1";
const EXPIRE: &str = "UPDATE fleet.approval_gates \
    SET expires_at=clock_timestamp()-interval '1 second' WHERE action_id=$1";
const PENDING: &str = "SELECT count(*) FROM fleet.approval_gates \
    WHERE fleet_id=$1 AND resolved_at IS NULL";

// Value: protects=deadline recovery and the last live matching blocker;
// fails_when=any expired gate admits a head still held by a live gate, or expired
// gates never requalify it; why_new=no sweeper runs; seam=real gate and queue rows.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
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
    let runs = fixtures.runs_with_dead_bus();
    let first = next(&runs, &runner).await.expect("initial head");
    let other = scoped_gate(&fixtures, &fleet, &workspace, &head).await;
    let specific = scoped_gate(&fixtures, &fleet, &workspace, &head).await;
    scoped_gate(&fixtures, &fleet, &workspace, &later).await;
    runs.park(&first.fleet_id, &head, first.fence)
        .await
        .expect("park on approvals");
    assert!(
        next(&runs, &runner).await.is_none(),
        "live gates block the head and tail"
    );
    assert_blocked_assignment(&fixtures, &fleet, &runner).await;
    let (early, last) = if first_gate_first {
        (&other, &specific)
    } else {
        (&specific, &other)
    };
    lapse(&fixtures, early).await;
    assert!(
        next(&runs, &runner).await.is_none(),
        "one matching gate is still live"
    );
    assert_blocked_assignment(&fixtures, &fleet, &runner).await;
    lapse(&fixtures, last).await;
    let resumed = next(&runs, &runner)
        .await
        .expect("all matching gates lapsed");
    assert_eq!(resumed.turn_id, head, "the later event's gate is unrelated");
    assert!(resumed.fence > first.fence);
    assert_answerable(&fixtures, &fleet, &specific).await;
    drop(runs);
    fixtures.cleanup().await;
}

async fn assert_answerable(fixtures: &Fixtures, fleet: &Uuid7, specific: &Uuid7) {
    let count: i64 = sqlx::query_scalar(PENDING)
        .bind(fleet)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("pending cards");
    assert_eq!(
        count, 3,
        "discovery and assignment do not expire approval cards"
    );
    approve(fixtures, specific).await;
}

async fn approve(fixtures: &Fixtures, action: &Uuid7) {
    let bus = afd_bus::test_util::unreachable();
    let inbox = Inbox::new(
        fixtures.database.clone(),
        bus.clone(),
        Turns::for_tests(fixtures.database.clone(), bus),
    );
    assert!(matches!(
        inbox
            .resolve(
                action.as_str(),
                Decision::Approved,
                "fixture:human",
                "",
                None
            )
            .await
            .expect("late answer"),
        Resolution::Resolved(_)
    ));
    drop(inbox);
}

// Value: protects=standing access cannot loop through assignment and park after its
// deadline; fails_when=deadline alone wakes it; seam=real approval and thread rows.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn standing_access_waits_for_an_answer_after_its_card_deadline() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let head = enqueue(&fixtures, &fleet, &workspace).await;
    let runs = fixtures.runs_with_dead_bus();
    let first = next(&runs, &runner).await.expect("initial head");
    let action = seed_gate(&fixtures, &fleet, &workspace).await;
    runs.park(&first.fleet_id, &head, first.fence)
        .await
        .expect("park on access");
    lapse(&fixtures, &action).await;
    assert!(
        next(&runs, &runner).await.is_none(),
        "only an answer resolves access"
    );
    assert_blocked_assignment(&fixtures, &fleet, &runner).await;
    approve(&fixtures, &action).await;
    assert_eq!(
        next(&runs, &runner).await.expect("answered access").turn_id,
        head
    );
    drop(runs);
    fixtures.cleanup().await;
}

async fn scoped_gate(
    fixtures: &Fixtures,
    fleet: &Uuid7,
    workspace: &Uuid7,
    event: &Uuid7,
) -> Uuid7 {
    let action = seed_gate(fixtures, fleet, workspace).await;
    sqlx::query(SCOPE)
        .bind(&action)
        .bind(event)
        .execute(&mut *fixtures.database.acquire().await.expect("gate scope"))
        .await
        .expect("event-specific gate");
    action
}

async fn lapse(fixtures: &Fixtures, action: &Uuid7) {
    sqlx::query(EXPIRE)
        .bind(action)
        .execute(&mut *fixtures.database.acquire().await.expect("gate clock"))
        .await
        .expect("deadline in the past");
}

async fn assert_blocked_assignment(fixtures: &Fixtures, fleet: &Uuid7, runner: &Uuid7) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("assignment connection");
    let before: i64 =
        sqlx::query_scalar("SELECT fencing_token FROM thread.threads WHERE fleet_id=$1")
            .bind(fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("thread before refusal");
    let mut tx = connection
        .begin()
        .await
        .expect("direct assignment transaction");
    let row = sqlx::query(ASSIGN_HEAD)
        .bind(fleet)
        .bind(runner)
        .bind(af_core::timing::RUN_TTL)
        .bind(af_core::timing::RUNNER_OFFLINE_AFTER)
        .bind(afd_state::sql::RUNNER_STATUS_ACTIVE)
        .bind(afd_state::sql::RUN_STATUS_ACTIVE)
        .bind(afd_state::sql::RUN_STATUS_EXPIRED)
        .bind(af_wire::task::CAPABILITY)
        .bind(af_wire::task::TaskState::Ready.as_str())
        .bind(af_wire::task::TaskState::Running.as_str())
        .bind(afd_fleet::task::DECISION_POLICY_PATH)
        .bind(afd_state::sql::FLEET_STATUS_ACTIVE)
        .bind(af_core::turn::status::RECEIVED)
        .bind(true)
        .bind(afd_credential::provider::PLATFORM_MODEL_REF)
        .fetch_optional(&mut *tx)
        .await
        .expect("assignment checks the same gate rule");
    assert!(row.is_none());
    tx.rollback()
        .await
        .expect("empty assignment rolls back the thread");
    let after: i64 =
        sqlx::query_scalar("SELECT fencing_token FROM thread.threads WHERE fleet_id=$1")
            .bind(fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("thread after refusal");
    assert_eq!(after, before);
}
