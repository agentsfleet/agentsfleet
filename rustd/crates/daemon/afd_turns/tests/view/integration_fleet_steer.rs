//! One fleet's message to another: written only while its guard holds, and a
//! repeat is answered its first turn even when the deployment ceiling is spent.
#![expect(clippy::expect_used, reason = "test assertions")]

use std::pin::Pin;

use afd_db::Precondition;
use afd_db::test_util::mint_id;
use afd_turns::view::{FleetSteer, Steer};
use afd_turns::{Budgets, Unheld};
use sqlx::PgConnection;

use crate::support::TurnsLane;

/// The stored body a fleet's steer carries.
const REQUEST_JSON: &str = r#"{"message":"run the hourly check"}"#;

/// A guard that answers its verdict every time it is asked.
struct Verdict(bool);

impl Precondition for Verdict {
    fn holds<'c>(
        &'c self,
        _connection: &'c mut PgConnection,
    ) -> Pin<Box<dyn Future<Output = sqlx::Result<bool>> + Send + 'c>> {
        Box::pin(std::future::ready(Ok(self.0)))
    }
}

/// How many admitted turns the lane's fleet holds.
async fn rows(lane: &TurnsLane) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM thread.turns WHERE producer IS NOT NULL AND thread_id = $1",
    )
    .bind(&lane.fleet)
    .fetch_one(&mut *lane.connection().await)
    .await
    .expect("the ledger answers")
}

/// The lane's fleet steering itself from `event`, one of its own turns.
fn steer<'a>(lane: &'a TurnsLane, event: &'a af_core::id::Uuid7) -> FleetSteer<'a> {
    FleetSteer {
        target: &lane.fleet,
        workspace: &lane.workspace,
        sender_fleet: &lane.fleet,
        sender_event: event,
        masked_text: "run the hourly check",
        request_json: REQUEST_JSON,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_steer_whose_guard_fails_writes_nothing() {
    let lane = TurnsLane::open().await;
    let event = mint_id();
    lane.seed_event(&event, jiff::Timestamp::now()).await;
    let before = rows(&lane).await;

    let answered = Steer::new(lane.turns())
        .append_from_fleet(steer(&lane, &event), &Verdict(false))
        .await
        .expect("an unheld guard is no error");

    assert_eq!(answered.err(), Some(Unheld));
    assert_eq!(rows(&lane).await, before, "a failed guard writes no row");
    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_repeated_fleet_steer_is_answered_its_turn_past_a_spent_ceiling() {
    let lane = TurnsLane::open().await;
    let event = mint_id();
    lane.seed_event(&event, jiff::Timestamp::now()).await;
    let first = Steer::new(lane.turns())
        .append_from_fleet(steer(&lane, &event), &Verdict(true))
        .await
        .expect("the first steer is admitted")
        .expect("the guard holds");
    assert!(first.admitted_at.is_some(), "this call admitted it");
    let before = rows(&lane).await;

    let spent = lane.turns().with_budgets(Budgets {
        deployment_backlog: 0,
    });
    let repeated = Steer::new(spent)
        .append_from_fleet(steer(&lane, &event), &Verdict(true))
        .await
        .expect("a repeat is answered whatever the ceiling")
        .expect("the guard holds");

    assert_eq!(repeated.turn_id, first.turn_id);
    assert_eq!(repeated.admitted_at, None, "a repeat admits nothing");
    assert_eq!(rows(&lane).await, before, "the repeat writes no row");
    lane.cleanup().await;
}

/// New words from a fleet past a spent deployment ceiling have no first turn
/// to answer with, so the capacity refusal stands and nothing is written.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_new_fleet_steer_past_a_spent_ceiling_is_refused_for_capacity() {
    let lane = TurnsLane::open().await;
    let event = mint_id();
    lane.seed_event(&event, jiff::Timestamp::now()).await;
    let before = rows(&lane).await;
    let spent = lane.turns().with_budgets(Budgets {
        deployment_backlog: 0,
    });

    let refused = Steer::new(spent)
        .append_from_fleet(steer(&lane, &event), &Verdict(true))
        .await
        .expect_err("no repeat to answer, so the ceiling refuses");

    assert!(
        refused.is_datastore_unavailable(),
        "capacity invites the retry an outage does: {refused}"
    );
    assert_eq!(refused.code(), af_core::error_code::INTERNAL_DB_UNAVAILABLE);
    assert_eq!(rows(&lane).await, before, "a refused steer writes no row");
    lane.cleanup().await;
}
