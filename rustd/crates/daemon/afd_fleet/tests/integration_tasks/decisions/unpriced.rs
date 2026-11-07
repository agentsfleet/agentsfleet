//! Unpriceable measured usage retains its frozen evidence and credit ceiling.
#![expect(
    clippy::panic,
    reason = "a provider fixture with another variant must fail loudly"
)]

use af_core::{error_code, test_util::trace::Capture};
use af_wire::{
    decision::DecisionUsage,
    task::{
        DecisionAssessment, DecisionCallState, DecisionNotSentCause, DecisionReceipt,
        DecisionReceiptInput, DecisionRecordDisposition,
    },
};
use afd_fleet::task::TaskLifecycle as _;
use serde_json::Value;

use super::{
    super::fixture::{Case, exercise, id},
    fixture::{Decisions, known, request},
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn unpriceable_usage_survives_catalogue_changes_and_conflicting_receipts_without_free_credit()
{
    let capture = Capture::install();
    for (rate, usage) in [
        (
            None,
            DecisionUsage {
                input_tokens: 1000,
                output_tokens: 100,
                cached_input_tokens: 200,
                cache_write_tokens: 100,
            },
        ),
        (
            Some(i64::MAX),
            DecisionUsage {
                input_tokens: 1_000_001,
                output_tokens: 0,
                cached_input_tokens: 0,
                cache_write_tokens: 0,
            },
        ),
    ] {
        exercise(async |case| retained(case, rate, usage, &capture).await).await;
    }
}

async fn retained(case: &Case, rate: Option<i64>, usage: DecisionUsage, capture: &Capture) {
    let fixture = Decisions::create(case).await;
    set_rate(&fixture, rate).await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let (_, body) = fixture.reserve(&input).await;
    let call = body
        .get("call_id")
        .and_then(Value::as_str)
        .expect("issued physical call");
    let mut receipt = known(&request(&body));
    let DecisionReceipt::Provider { result } = &mut receipt else {
        panic!("known provider fixture")
    };
    result.usage = Some(usage);
    assert_unpriced(&fixture, call, &receipt, usage, false).await;
    let saved = snapshot(&fixture, call).await;
    assert!(fixture.record(call, receipt.clone()).await.replayed);
    contradictory_receipts(&fixture, call, &receipt).await;
    refuse_retry(&fixture, &input, call).await;
    fixture.reprice().await;
    assert_unpriced(&fixture, call, &receipt, usage, true).await;
    assert_eq!(snapshot(&fixture, call).await, saved);
    assert_eq!(fixture.counts().await, (1, 1, 0));
    assert!(!fixture.reserve(&input).await.0);
    if rate.is_some() {
        overflow_diagnostics(capture, call);
    }
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn set_rate(fixture: &Decisions<'_>, rate: Option<i64>) {
    if let Some(rate) = rate {
        let mut details = fixture.model.model.details.clone();
        details
            .decision
            .as_mut()
            .expect("reviewed decision model")
            .prices
            .first_mut()
            .expect("dated decision tier")
            .input_nanos_per_mtok = rate;
        assert!(
            fixture
                .model
                .models
                .update(&fixture.model.model.id, details, af_core::clock::now())
                .await
                .expect("publish a valid signed-64-bit price")
        );
    }
}

async fn assert_unpriced(
    fixture: &Decisions<'_>,
    call: &str,
    receipt: &DecisionReceipt,
    usage: DecisionUsage,
    replayed: bool,
) {
    let recorded = fixture.record(call, receipt.clone()).await;
    assert_eq!(
        (
            recorded.state,
            recorded.disposition,
            recorded.replayed,
            recorded.usage,
            recorded.charged_nanos,
            recorded.overage_nanos,
            recorded.reserved_nanos.as_str()
        ),
        (
            DecisionCallState::Unknown,
            DecisionRecordDisposition::Unusable,
            replayed,
            Some(usage),
            None,
            None,
            "20000000"
        )
    );
    assert_eq!(fixture.counts().await, (1, 1, 0));
}

async fn refuse_retry(
    fixture: &Decisions<'_>,
    input: &garde::Valid<af_wire::task::DecisionReservationInput>,
    call: &str,
) {
    let mut retry = (**input).clone();
    retry.operation_id = afd_db::test_util::mint_id();
    retry.retry_of = Some(call.to_owned());
    let retry = garde::Unvalidated::new(retry)
        .validate()
        .expect("bounded retry request");
    let refused = fixture
        .tasks
        .reserve_decision(fixture.case.lease(), &retry)
        .await
        .expect_err("unpriced usage is not throttling evidence");
    assert_eq!(refused.code(), error_code::TASK_DECISION_UNAVAILABLE);
}

async fn contradictory_receipts(fixture: &Decisions<'_>, call: &str, receipt: &DecisionReceipt) {
    let mut changed = receipt.clone();
    let DecisionReceipt::Provider { result } = &mut changed else {
        panic!("known provider fixture")
    };
    result.usage.as_mut().expect("measured usage").output_tokens += 1;
    for receipt in [
        changed,
        DecisionReceipt::NotSent {
            cause: DecisionNotSentCause::Build,
        },
    ] {
        let input = garde::Unvalidated::new(DecisionReceiptInput {
            fencing_token: fixture.case.lease().fencing_token,
            receipt,
        })
        .validate()
        .expect("bounded contradictory receipt");
        let refused = fixture
            .tasks
            .record_decision(fixture.case.lease(), &id(call), &input)
            .await
            .expect_err("canonical unpriced evidence is immutable");
        assert_eq!(refused.code(), error_code::TASK_REPLAY_CONFLICT);
    }
}

async fn snapshot(fixture: &Decisions<'_>, call: &str) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('call',to_jsonb(c),'payload',to_jsonb(p)) FROM runner.decision_calls c JOIN runner.decision_payloads p ON p.decision_call_id=c.id WHERE c.id=$1::uuid")
        .bind(call).fetch_one(&mut *fixture.case.held.fixtures.database.acquire().await.expect("native evidence connection"))
        .await.expect("full durable call and payload")
}

fn overflow_diagnostics(capture: &Capture, call: &str) {
    let events = capture.events();
    let overflow = events
        .iter()
        .filter(|event| event.field("event") == Some("dt_decision_price_unavailable"))
        .collect::<Vec<_>>();
    assert_ne!(
        overflow.len(),
        0,
        "the native rater must report unrepresentable charges"
    );
    for event in overflow {
        assert_eq!(event.field("reason"), Some("charge_unrepresentable"));
        assert_eq!(
            event.field("error_code"),
            Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
        );
        assert_eq!(event.field("decision_call_id"), Some(call));
    }
}
