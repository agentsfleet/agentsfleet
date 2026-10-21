//! Dimension 3.2 — a full datastore produces classified backpressure and no
//! silent drop: a fleet holding its budget of outstanding entries, or a
//! deployment holding its budget of unsettled admissions, refuses the
//! producer with the capacity class and commits nothing.
//!
//! The budgets are set small through `Admissions::with_budgets`; the
//! production numbers are compile-time assertions in `afd_admission::budget`
//! and are not what this proves. What it proves is the refusal's shape, and
//! that the row the refusal protects Postgres from is genuinely absent.
//!
//! Marked `#[ignore]` so `make test-unit-all` compiles and lints this without
//! datastores; `make test-integration-rustd` runs it.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_admission::{Admission, Admissions, Budgets, Key, Producer};
use afd_core::error_code;
use afd_wire::event::EventType;

use crate::support::EventsLane;

/// How many entries the fleet may hold outstanding before it is refused.
const FLEET_BUDGET: u64 = 3;

/// A delivery key no earlier run holds.
///
/// A repeated key is unique per producer, not per fleet, so a fixed name
/// admitted by a run against a database the lane did not reset
/// (`KEEP_TEST_STATE=1`) would read as already admitted. The lane's fleet is
/// minted per run, so prefixing it keeps each run's keys its own.
fn delivery(lane: &EventsLane, name: &str) -> String {
    format!("{}-{name}", lane.fleet)
}

/// One webhook admission for the lane's fleet, keyed by `delivery`.
fn admission<'a>(lane: &'a EventsLane, delivery: &'a str) -> Admission<'a> {
    Admission {
        producer: Producer::Webhook,
        key: Key::Repeated(delivery),
        fleet: &lane.fleet,
        workspace: &lane.workspace,
        actor: "webhook:budget",
        event_type: EventType::Webhook,
        request_json: r#"{"delivery":"budget"}"#,
        reply: afd_admission::Reply::None,
    }
}

/// How many ledger rows the lane's fleet holds.
async fn rows_for(lane: &EventsLane) -> i64 {
    let mut connection = lane.connection().await;
    sqlx::query_scalar("SELECT count(*) FROM fleet.admissions WHERE fleet_id = $1::uuid")
        .bind(lane.fleet.as_str())
        .fetch_one(&mut *connection)
        .await
        .expect("the ledger answers")
}

/// The one shape every capacity refusal has: retryable to the caller,
/// capacity to the operator.
fn assert_capacity_refusal(refused: &afd_admission::Error) {
    assert!(
        refused.is_over_capacity(),
        "not the capacity class: {refused}"
    );
    assert!(
        refused.is_datastore_unavailable(),
        "capacity must invite the retry an outage does: {refused}"
    );
    assert_eq!(refused.code(), error_code::INTERNAL_DB_UNAVAILABLE);
}

/// Dimension 3.2, the fleet budget: unsettled admissions.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_holding_its_budget_of_outstanding_entries_refuses_the_producer() {
    let lane = EventsLane::open().await;
    let ledger = lane.admissions().with_budgets(Budgets {
        fleet_backlog: FLEET_BUDGET,
        deployment_backlog: u64::MAX,
    });

    for index in 0..FLEET_BUDGET {
        let key = delivery(&lane, &format!("within-{index}"));
        ledger
            .admit(admission(&lane, &key))
            .await
            .expect("an admission inside the budget is taken");
    }
    let before = rows_for(&lane).await;

    let refused = ledger
        .admit(admission(&lane, &delivery(&lane, "one-over")))
        .await
        .expect_err("the fleet holds its budget");
    assert_capacity_refusal(&refused);
    assert_eq!(
        rows_for(&lane).await,
        before,
        "a refusal commits nothing: the row is what the budget protects"
    );

    let mut connection = lane.connection().await;
    sqlx::query("UPDATE fleet.admissions SET settled_at = now() WHERE id = (SELECT id FROM fleet.admissions WHERE fleet_id = $1::uuid ORDER BY created_at, sequence LIMIT 1)")
        .bind(&lane.fleet)
        .execute(&mut *connection)
        .await
        .expect("settle the oldest admission");
    drop(connection);
    ledger
        .admit(admission(&lane, &delivery(&lane, "one-over")))
        .await
        .expect("room was made");

    lane.cleanup().await;
}

/// Dimension 3.2, the deployment budget: unsettled rows.
///
/// The budget is a deployment-wide count that other suites in this lane can
/// raise concurrently, so the refusal is proven with a budget of zero — under
/// which nothing new is ever admitted — rather than by counting to a limit
/// another test could cross first. The retry exception is proven beside it:
/// work already admitted is answered its id however deep the backlog is.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_spent_deployment_budget_refuses_new_work_and_still_answers_a_retry() {
    let lane = EventsLane::open().await;
    let open = lane.admissions();
    let admitted = open
        .admit(admission(&lane, &delivery(&lane, "already-admitted")))
        .await
        .expect("admitted while the budget is open");

    let spent = lane.admissions().with_budgets(Budgets {
        fleet_backlog: u64::MAX,
        deployment_backlog: 0,
    });
    let before = rows_for(&lane).await;
    let refused = spent
        .admit(admission(&lane, &delivery(&lane, "new-work")))
        .await
        .expect_err("a spent deployment budget refuses new work");
    assert_capacity_refusal(&refused);
    assert_eq!(rows_for(&lane).await, before, "a refusal commits nothing");

    let retried = spent
        .admit(admission(&lane, &delivery(&lane, "already-admitted")))
        .await
        .expect("a retry of admitted work is answered whatever the budget");
    assert!(retried.replayed, "the retry is recognised as one");
    assert_eq!(
        retried.stored.id, admitted.stored.id,
        "and answered its original id"
    );

    lane.cleanup().await;
}

/// The ledger's own backlog figure: rows the queue has not confirmed, and
/// how long the oldest has waited. Read whole, so it is a lower bound over
/// what this test deferred.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_ledger_reports_its_unsettled_rows_and_the_age_of_the_oldest() {
    let lane = EventsLane::open().await;
    // Notifications can fail without changing acceptance or backlog.
    let unreachable = afd_dragonfly::Dragonfly::unreachable(
        &afd_dragonfly::DragonflyConfig::from_url(
            afd_dragonfly::DragonflyRole::Default,
            "redis://127.0.0.1:1".to_owned(),
        )
        .with_request_timeout(std::time::Duration::from_millis(200)),
    )
    .expect("a pending handle is built without a socket");
    let deferring = Admissions::for_tests(lane.database.clone(), unreachable);
    deferring
        .admit(admission(&lane, &delivery(&lane, "deferred")))
        .await
        .expect("the row commits whatever the queue does");

    let backlog = lane
        .admissions()
        .backlog()
        .await
        .expect("the ledger answers");
    assert!(
        backlog.rows >= 1,
        "the deferred row is counted: {backlog:?}"
    );
    assert!(
        backlog.oldest_age.is_some(),
        "a non-empty backlog has an oldest row: {backlog:?}"
    );

    lane.cleanup().await;
}
