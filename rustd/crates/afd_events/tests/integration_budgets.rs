//! Dimension 3.2 — a full datastore produces classified backpressure and no
//! silent drop: a fleet holding its budget of outstanding entries, or a
//! deployment holding its budget of unconfirmed admissions, refuses the
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
use afd_core::clock;
use afd_core::error_code;
use afd_dragonfly::streams::FleetStreams;
use afd_wire::event::EventType;
use sqlx::Row as _;

use crate::support::EventsLane;

/// How many entries the fleet may hold outstanding before it is refused.
const FLEET_BUDGET: u64 = 3;

/// The consumer the test drains one entry under.
const CONSUMER: &str = "budget-reader";

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
    sqlx::query("SELECT count(*) FROM core.fleet_admissions WHERE fleet_id = $1::uuid")
        .bind(lane.fleet.as_str())
        .fetch_one(&mut *connection)
        .await
        .expect("the ledger answers")
        .try_get(0)
        .expect("count answers a bigint")
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

/// Dimension 3.2, the fleet budget: outstanding entries on the stream.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_holding_its_budget_of_outstanding_entries_refuses_the_producer() {
    let lane = EventsLane::open().await;
    let streams = FleetStreams::new(lane.queue.clone());
    streams
        .ensure_group(&lane.fleet)
        .await
        .expect("group create");
    let ledger = lane.admissions().with_budgets(Budgets {
        fleet_backlog: FLEET_BUDGET,
        replay_backlog: u64::MAX,
    });

    for index in 0..FLEET_BUDGET {
        let delivery = format!("within-{index}");
        ledger
            .admit(admission(&lane, &delivery))
            .await
            .expect("an admission inside the budget is taken");
    }
    let before = rows_for(&lane).await;

    let refused = ledger
        .admit(admission(&lane, "one-over"))
        .await
        .expect_err("the fleet holds its budget");
    assert_capacity_refusal(&refused);
    assert_eq!(
        rows_for(&lane).await,
        before,
        "a refusal commits nothing: the row is what the budget protects"
    );

    // Draining one entry makes room for exactly one more.
    let event = streams
        .read_new(&lane.fleet, CONSUMER)
        .await
        .expect("read")
        .expect("an entry is outstanding");
    assert!(streams.ack(&lane.fleet, &event.receipt).await.expect("ack"));
    ledger
        .admit(admission(&lane, "one-over"))
        .await
        .expect("room was made");

    streams.forget(&lane.fleet).await.expect("cleanup");
    lane.cleanup().await;
}

/// Dimension 3.2, the deployment budget: rows the queue never confirmed.
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
    FleetStreams::new(lane.queue.clone())
        .ensure_group(&lane.fleet)
        .await
        .expect("group create");
    let open = lane.admissions();
    let admitted = open
        .admit(admission(&lane, "already-admitted"))
        .await
        .expect("admitted while the budget is open");

    let spent = lane.admissions().with_budgets(Budgets {
        fleet_backlog: u64::MAX,
        replay_backlog: 0,
    });
    let before = rows_for(&lane).await;
    let refused = spent
        .admit(admission(&lane, "new-work"))
        .await
        .expect_err("a spent deployment budget refuses new work");
    assert_capacity_refusal(&refused);
    assert_eq!(rows_for(&lane).await, before, "a refusal commits nothing");

    let retried = spent
        .admit(admission(&lane, "already-admitted"))
        .await
        .expect("a retry of admitted work is answered whatever the budget");
    assert!(retried.replayed, "the retry is recognised as one");
    assert_eq!(retried.id, admitted.id, "and answered its original id");

    FleetStreams::new(lane.queue.clone())
        .forget(&lane.fleet)
        .await
        .expect("cleanup");
    lane.cleanup().await;
}

/// The ledger's own backlog figure: rows the queue has not confirmed, and
/// how long the oldest has waited. Read whole, so it is a lower bound over
/// what this test deferred.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_ledger_reports_its_unconfirmed_rows_and_the_age_of_the_oldest() {
    let lane = EventsLane::open().await;
    // A queue nothing listens on: every append is deferred, so the row is
    // committed and the receipt stays NULL.
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
        .admit(admission(&lane, "deferred"))
        .await
        .expect("the row commits whatever the queue does");

    let backlog = lane
        .admissions()
        .backlog(clock::now())
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
