//! Uncertainty retains credit; a late known result accounts exactly once.

use afd_core::error_code;
use afd_fleet::task::TaskLifecycle as _;
use afd_wire::task::{
    DecisionAssessment, DecisionCallState, DecisionNotSentCause, DecisionReceipt,
    DecisionReceiptInput, DecisionRetryDelay, DecisionUnavailableCause,
};
use serde_json::{Value, json};

use super::{
    super::fixture::{Case, exercise, id},
    fixture::{Decisions, known, request},
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_physical_timeout_cannot_turn_into_retry_evidence_or_free_usage() {
    exercise(uncertain).await;
}

async fn uncertain(case: &Case) {
    let fixture = Decisions::create(case).await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let (_, body) = fixture.reserve(&input).await;
    let call = body
        .get("call_id")
        .and_then(Value::as_str)
        .expect("native call");
    let unknown = DecisionReceipt::Unknown {
        cause: DecisionUnavailableCause::Timeout,
    };
    let recorded = fixture.record(call, unknown.clone()).await;
    assert_eq!(
        (
            recorded.state,
            recorded.charged_nanos,
            recorded.usage,
            recorded.reserved_nanos.as_str()
        ),
        (DecisionCallState::Unknown, None, None, "20000000")
    );
    assert!(fixture.record(call, unknown).await.replayed);
    assert_eq!(fixture.counts().await, (1, 1, 0));
    immutable_uncertainty(&fixture, &input, call).await;
    let receipt = known(&request(&body));
    let known = fixture.record(call, receipt.clone()).await;
    assert_eq!(
        (
            known.state,
            known.charged_nanos.as_deref(),
            known.reserved_nanos.as_str()
        ),
        (DecisionCallState::Known, Some("3"), "0")
    );
    assert!(fixture.record(call, receipt).await.replayed);
    assert_eq!(fixture.counts().await, (1, 1, 1));
    fixture.model.finish().await;
}

async fn immutable_uncertainty(
    fixture: &Decisions<'_>,
    input: &garde::Valid<afd_wire::task::DecisionReservationInput>,
    call: &str,
) {
    for receipt in [
        DecisionReceipt::NotSent {
            cause: DecisionNotSentCause::Build,
        },
        DecisionReceipt::Unknown {
            cause: DecisionUnavailableCause::ProviderStatus {
                status: 429,
                retry_delay: DecisionRetryDelay::Unspecified,
            },
        },
    ] {
        let input = garde::Unvalidated::new(DecisionReceiptInput {
            fencing_token: fixture.case.lease().fencing_token,
            receipt,
        })
        .validate()
        .expect("bounded conflicting evidence");
        let fault = fixture
            .tasks
            .record_decision(fixture.case.lease(), &id(call), &input)
            .await
            .expect_err("first uncertainty is immutable");
        assert_eq!(fault.code(), error_code::TASK_REPLAY_CONFLICT);
    }
    let mut retry = (**input).clone();
    retry.operation_id = afd_db::test_util::mint_id();
    retry.retry_of = Some(call.to_owned());
    let retry = garde::Unvalidated::new(retry)
        .validate()
        .expect("bounded requested retry");
    let fault = fixture
        .tasks
        .reserve_decision(fixture.case.lease(), &retry)
        .await
        .expect_err("timeout is not throttling proof");
    assert_eq!(fault.code(), error_code::TASK_DECISION_UNAVAILABLE);
    assert_eq!(fixture.counts().await, (1, 1, 0));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_throttled_retry_preserves_request_quote_deadline_and_one_attempt() {
    exercise(throttled).await;
}

async fn throttled(case: &Case) {
    let fixture = Decisions::create(case).await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let (_, initial) = fixture.reserve(&input).await;
    let call = initial
        .get("call_id")
        .and_then(Value::as_str)
        .expect("first call");
    fixture
        .record(
            call,
            DecisionReceipt::Unknown {
                cause: DecisionUnavailableCause::ProviderStatus {
                    status: 529,
                    retry_delay: DecisionRetryDelay::Unspecified,
                },
            },
        )
        .await;
    let mut retry = (*input).clone();
    retry.operation_id = afd_db::test_util::mint_id();
    retry.retry_of = Some(call.to_owned());
    let retry = garde::Unvalidated::new(retry)
        .validate()
        .expect("permitted retry shape");
    let (issued, second) = fixture.reserve(&retry).await;
    assert!(issued);
    same_assessment(&initial, &second);
    assert!(
        !fixture.reserve(&retry).await.0,
        "the second physical key is not replayed"
    );
    assert_eq!(fixture.counts().await, (2, 2, 0));
    let second_id = second
        .get("call_id")
        .and_then(Value::as_str)
        .expect("second call");
    fixture.record(second_id, known(&request(&second))).await;
    assert_eq!(fixture.counts().await, (2, 2, 1));
    fixture.model.finish().await;
}

fn same_assessment(initial: &Value, second: &Value) {
    assert_eq!(
        second.pointer("/dispatch/expires_at"),
        initial.pointer("/dispatch/expires_at")
    );
    assert_eq!(
        second.pointer("/dispatch/assessment_id"),
        initial.pointer("/dispatch/assessment_id")
    );
    assert_eq!(second.pointer("/dispatch/attempt"), Some(&json!(1)));
    let first_request = request(initial);
    let mut second_request = request(second);
    assert_ne!(second_request.call_id, first_request.call_id);
    second_request.call_id.clone_from(&first_request.call_id);
    assert_eq!(second_request, first_request);
}
