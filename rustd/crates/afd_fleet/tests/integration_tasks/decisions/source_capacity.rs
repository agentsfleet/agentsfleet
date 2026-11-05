//! The physical source ceiling includes permitted retries and survives new context.

use afd_core::error_code;
use afd_fleet::task::TaskLifecycle as _;
use afd_wire::task::{
    DecisionAssessment, DecisionCallState, DecisionNotSentCause, DecisionReceipt,
    DecisionReservationInput, DecisionRetryDelay, DecisionUnavailableCause,
};

use super::{
    super::fixture::{Case, GOALS, exercise},
    fixture::{Decisions, request},
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn source_physical_limit_counts_each_retry_and_refuses_changed_context_without_mutation() {
    exercise(capacity).await;
}

async fn capacity(case: &Case) {
    let fixture = Decisions::create(case).await;
    let balance = fixture.wallet().await;
    attempts(
        &fixture,
        fixture.reservation(DecisionAssessment::Message, None),
    )
    .await;
    for ordinal in 0..4 {
        let item = fixture.reservation(
            DecisionAssessment::Item {
                ordinal,
                interpretation_digest: "2".repeat(64),
                source_start: 0,
                source_end: i32::try_from(GOALS.first().expect("first accepted action").len())
                    .expect("small source"),
            },
            None,
        );
        attempts(&fixture, item).await;
    }
    let task = fixture.bind_goal().await;
    let task_before = fixture.goal_snapshot(&task).await;
    let before = calls(&fixture).await;
    let next = fixture.reservation(DecisionAssessment::Message, Some(task.clone()));
    let fault = fixture
        .tasks
        .reserve_decision(case.lease(), &next)
        .await
        .expect_err("new native task context cannot replenish source calls");
    assert_eq!(fault.code(), error_code::TASK_LIMIT_EXHAUSTED);
    assert_eq!(calls(&fixture).await, before);
    assert_eq!(fixture.goal_snapshot(&task).await, task_before);
    assert_eq!(fixture.counts().await, (10, 10, 0));
    assert_eq!(fixture.wallet().await, balance);
    let outstanding: (i64,i64) = sqlx::query_as("SELECT count(*),SUM(reserved_nanos)::bigint FROM runner.decision_calls WHERE original_lease_id=$1::uuid AND accounted_at IS NULL")
        .bind(case.held.lease_key.as_str()).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("source reservation observer")).await.expect("five uncertain original sends");
    assert_eq!(outstanding, (5, 100_000_000));
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn attempts(fixture: &Decisions<'_>, input: garde::Valid<DecisionReservationInput>) {
    let (_, body) = fixture.reserve(&input).await;
    let first = request(&body);
    let uncertain = fixture
        .record(
            &first.call_id,
            DecisionReceipt::Unknown {
                cause: DecisionUnavailableCause::ProviderStatus {
                    status: 429,
                    retry_delay: DecisionRetryDelay::Unspecified,
                },
            },
        )
        .await;
    assert_eq!(
        (uncertain.state, uncertain.reserved_nanos.as_str()),
        (DecisionCallState::Unknown, "20000000")
    );
    let mut retry = (*input).clone();
    retry.operation_id = afd_db::test_util::mint_id();
    retry.retry_of = Some(first.call_id.clone());
    let retry = garde::Unvalidated::new(retry)
        .validate()
        .expect("permitted single retry");
    let (_, body) = fixture.reserve(&retry).await;
    let second = request(&body);
    assert_ne!(first.call_id, second.call_id);
    assert_eq!(
        (first.questions, first.state),
        (second.questions, second.state)
    );
    let written = fixture
        .record(
            &second.call_id,
            DecisionReceipt::NotSent {
                cause: DecisionNotSentCause::Build,
            },
        )
        .await;
    assert_eq!(
        (
            written.state,
            written.usage,
            written.charged_nanos.as_deref(),
            written.reserved_nanos.as_str()
        ),
        (DecisionCallState::NotSent, None, Some("0"), "0")
    );
}

async fn calls(fixture: &Decisions<'_>) -> Vec<(serde_json::Value, serde_json::Value)> {
    sqlx::query_as("SELECT to_jsonb(c),to_jsonb(p) FROM runner.decision_calls c JOIN runner.decision_payloads p ON p.decision_call_id=c.id WHERE c.original_lease_id=$1::uuid ORDER BY c.id")
        .bind(fixture.case.held.lease_key.as_str())
        .fetch_all(&mut *fixture.case.held.fixtures.database.acquire().await.expect("all source calls observer"))
        .await.expect("complete calls and payloads")
}
