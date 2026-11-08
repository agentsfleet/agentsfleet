//! Exact monetary bounds refund unsent work without replenishing call allowances.

use af_core::error_code;
use af_wire::task::{
    DecisionAssessment, DecisionCallState, DecisionNotSentCause, DecisionReceipt,
    DecisionRecordDisposition, DecisionReservationInput, TaskLimits,
};
use afd_fleet::task::TaskLifecycle as _;

use super::{
    super::fixture::{Case, GOALS, exercise},
    fixture::{Decisions, request},
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn fleet_and_task_boundaries_refund_unsent_credit_without_resetting_call_history() {
    exercise(fleet).await;
    exercise(task_money).await;
    exercise(task_calls).await;
}

async fn fleet(case: &Case) {
    let fixture = Decisions::with_daily_budget(case, 20_000_000).await;
    let balance = fixture.wallet().await;
    let (_, body) = fixture
        .reserve(&fixture.reservation(DecisionAssessment::Message, None))
        .await;
    let first = request(&body);
    let next = fixture.reservation(
        DecisionAssessment::Item {
            ordinal: 0,
            interpretation_digest: "1".repeat(64),
            source_start: 0,
            source_end: i32::try_from(GOALS.first().expect("first action").len())
                .expect("small action"),
        },
        None,
    );
    let before = fixture.call_snapshot().await;
    denied(&fixture, &next).await;
    assert_eq!(fixture.call_snapshot().await, before);
    assert_eq!(fixture.counts().await, (1, 1, 0));
    unsent(&fixture, &first.call_id).await;
    let (_, body) = fixture.reserve(&next).await;
    let second = request(&body);
    assert_ne!(first.call_id, second.call_id);
    unsent(&fixture, &second.call_id).await;
    assert_eq!(fixture.wallet().await, balance);
    assert_eq!(fixture.counts().await, (2, 2, 0));
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn task_money(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture
        .bind_goal_with_limits(TaskLimits {
            model_nanos: 20_000_000,
            ..TaskLimits::default()
        })
        .await;
    let initial = fixture.goal_snapshot(&task).await;
    let balance = fixture.wallet().await;
    let (_, body) = fixture.reserve(&fixture.goal_reservation(&task)).await;
    let first = request(&body);
    let next = fixture.goal_reservation(&task);
    let reserved = fixture.goal_snapshot(&task).await;
    denied(&fixture, &next).await;
    assert_eq!(fixture.goal_snapshot(&task).await, reserved);
    unsent(&fixture, &first.call_id).await;
    let (_, body) = fixture.reserve(&next).await;
    unsent(&fixture, &request(&body).call_id).await;
    let mut expected = initial;
    expected.context.usage.decision_calls = 2;
    expected.resource_version += 4;
    assert_eq!(fixture.goal_snapshot(&task).await, expected);
    assert_eq!(fixture.wallet().await, balance);
    assert_eq!(fixture.counts().await, (2, 2, 0));
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn task_calls(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture
        .bind_goal_with_limits(TaskLimits {
            decision_calls: 1,
            ..TaskLimits::default()
        })
        .await;
    let initial = fixture.goal_snapshot(&task).await;
    let balance = fixture.wallet().await;
    let (_, body) = fixture.reserve(&fixture.goal_reservation(&task)).await;
    unsent(&fixture, &request(&body).call_id).await;
    let mut expected = initial;
    expected.context.usage.decision_calls = 1;
    expected.resource_version += 2;
    assert_eq!(fixture.goal_snapshot(&task).await, expected);
    denied(&fixture, &fixture.goal_reservation(&task)).await;
    assert_eq!(fixture.goal_snapshot(&task).await, expected);
    assert_eq!(fixture.wallet().await, balance);
    assert_eq!(fixture.counts().await, (1, 1, 0));
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn denied(fixture: &Decisions<'_>, input: &garde::Valid<DecisionReservationInput>) {
    let fault = fixture
        .tasks
        .reserve_decision(fixture.case.lease(), input)
        .await
        .expect_err("exact cumulative allowance blocks new work");
    assert_eq!(fault.code(), error_code::TASK_LIMIT_EXHAUSTED);
}

async fn unsent(fixture: &Decisions<'_>, call: &str) {
    let receipt = DecisionReceipt::NotSent {
        cause: DecisionNotSentCause::Build,
    };
    let written = fixture.record(call, receipt.clone()).await;
    assert_eq!(
        (
            written.state,
            written.disposition,
            written.charged_nanos.as_deref(),
            written.usage,
            written.reserved_nanos.as_str()
        ),
        (
            DecisionCallState::NotSent,
            DecisionRecordDisposition::Unusable,
            Some("0"),
            None,
            "0"
        )
    );
    let mut expected = written;
    expected.replayed = true;
    assert_eq!(fixture.record(call, receipt).await, expected);
}
