//! A reservation is refused, and nothing is reserved, when its fleet, its task
//! or its own saved call can no longer stand behind a paid decision.

use af_core::{error_code, error_code::ErrorCode};
use af_wire::durable_task::DecisionReservationInput;
use afd_fleet::durable_task::DurableTaskLifecycle as _;

use super::{
    super::fixture::{Case, exercise},
    fixture::Decisions,
};

async fn write(case: &Case, statement: &'static str) {
    sqlx::query(statement)
        .bind(&case.held.fleet)
        .execute(&mut *case.held.fixtures.database.acquire().await.expect("writer"))
        .await
        .expect("the fixture row changes");
}

async fn refused(
    fixture: &Decisions<'_>,
    input: &garde::Valid<DecisionReservationInput>,
    code: ErrorCode,
) {
    let failure = fixture
        .tasks
        .reserve_decision(fixture.case.run(), input)
        .await
        .expect_err("no decision is reserved");
    assert_eq!(failure.code(), code, "{failure}");
}

// Value: protects=a fleet that stopped being active opens no paid decision;
// fails_when=the fleet-status check is dropped from the reservation's context;
// why_new=the refusal had no case; seam=the production reserve verb over live
// Postgres, the fleet archived under its held run; no_claim=assignment, which
// stops offering an archived fleet.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_archived_fleet_reserves_no_decision() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        write(
            case,
            "UPDATE fleet.fleets SET status='archived' WHERE id=$1",
        )
        .await;
        refused(
            &fixture,
            &fixture.call_input().await,
            error_code::DURABLE_TASK_STALE_STATE,
        )
        .await;
        assert_eq!(fixture.counts().await, (0, 0, 0));
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

// Value: protects=a goal decision is reserved only for a live task;
// fails_when=the expiry check is dropped from goal preparation; seam=the
// production bind and reserve verbs over live Postgres; no_claim=the
// running-state check, which the run's lock refuses first.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_goal_decision_needs_a_live_durable_task() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;
        write(
            case,
            "UPDATE fleet.durable_tasks SET expires_at = created_at WHERE fleet_id=$1",
        )
        .await;
        refused(
            &fixture,
            &fixture.goal_reservation(&task),
            error_code::DURABLE_TASK_STALE_STATE,
        )
        .await;
        assert_eq!(fixture.counts().await, (0, 0, 0));
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

// Value: protects=a saved call whose stored status no build wrote is refused as
// corrupt, never classified as reusable; fails_when=the status decoder accepts or
// skips an unknown status; why_new=the decoder's refusal had no case; seam=the
// production reserve verb replaying its own operation over live Postgres, the
// call's status overwritten; no_claim=how the corruption arose.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_saved_call_with_an_unknown_status_is_refused_as_corrupt() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let input = fixture.call_input().await;
        fixture.reserve(&input).await;
        sqlx::query("UPDATE runner.decision_calls SET status='mislaid' WHERE original_run_id=$1")
            .bind(&case.held.run_key)
            .execute(&mut *case.held.fixtures.database.acquire().await.expect("writer"))
            .await
            .expect("the call's status is overwritten");
        refused(&fixture, &input, error_code::INTERNAL_DB_QUERY).await;
        assert_eq!(fixture.counts().await, (1, 1, 0), "the call is kept");
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}
