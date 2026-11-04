//! Item judgments retain exact human text without funding invalid byte ranges.

use std::ops::Range;

use afd_core::{error_code, id::Uuid7};
use afd_fleet::task::TaskLifecycle as _;
use afd_wire::task::{
    DecisionAssessment, DecisionCallState, DecisionRecordDisposition, DecisionReservationInput,
};
use serde_json::{Value, json};

use super::{
    super::fixture::{Case, exercise},
    fixture::{Decisions, known, request},
};

const MESSAGE: &str = "Review PR 101: café 🦉; keep middleware unchanged.";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn item_decisions_preserve_unicode_and_refuse_invalid_source_ranges_before_funding() {
    exercise(selected).await;
}

async fn selected(case: &Case) {
    let fixture = Decisions::create(case).await;
    let source = case
        .source_message(
            afd_admission::Producer::Steer,
            &afd_events::steer_actor("native_decision_source"),
            MESSAGE,
        )
        .await;
    refused_ranges(&fixture, &source).await;
    let input = item(&fixture, &source, 15..25);
    let (issued, body) = fixture.reserve(&input).await;
    assert!(issued);
    let request = request(&body);
    assert_item(&request, &source);
    settled_replay(&fixture, &input, &request).await;
    fixture.model.finish().await;
}

async fn refused_ranges(fixture: &Decisions<'_>, source: &Uuid7) {
    let admitted = source_snapshot(fixture, source).await;
    for (start, end) in [(26, 25), (0, i32::MAX), (19, 25), (15, 23)] {
        let input = item(fixture, source, start..end);
        let failure = fixture
            .tasks
            .reserve_decision(fixture.case.lease(), &input)
            .await
            .expect_err("reversed, oversized and split-Unicode ranges cannot fund a call");
        assert_eq!(failure.code(), error_code::TASK_INVALID_TARGET);
        assert_eq!(fixture.counts().await, (0, 0, 0));
        assert_eq!(source_snapshot(fixture, source).await, admitted);
    }
}

fn assert_item(request: &afd_wire::decision::DecisionRequest, source: &Uuid7) {
    assert_eq!(request.state.get("message"), Some(&json!(MESSAGE)));
    assert_eq!(
        request.state.get("assessed_message"),
        Some(&json!("café 🦉"))
    );
    assert_eq!(
        request.state.get("source_admission_id"),
        Some(&json!(source.as_str()))
    );
    assert_eq!(
        request.state.get("assessment"),
        Some(&json!({
            "kind":"item", "ordinal":0, "interpretation_digest":"a".repeat(64),
            "source_start":15, "source_end":25,
        }))
    );
    assert_eq!(
        request
            .questions
            .iter()
            .map(|q| q.name.as_str())
            .collect::<Vec<_>>(),
        ["intent", "association"]
    );
}

async fn settled_replay(
    fixture: &Decisions<'_>,
    input: &garde::Valid<DecisionReservationInput>,
    request: &afd_wire::decision::DecisionRequest,
) {
    let recorded = fixture.record(&request.call_id, known(request)).await;
    assert_eq!(
        (
            recorded.state,
            recorded.disposition,
            recorded.charged_nanos.as_deref()
        ),
        (
            DecisionCallState::Known,
            DecisionRecordDisposition::Current,
            Some("3")
        )
    );
    assert_eq!(fixture.counts().await, (1, 1, 1));
    let (issued, replay) = fixture.reserve(input).await;
    assert!(!issued);
    assert_eq!(
        replay.pointer("/recorded/request"),
        Some(&serde_json::to_value(request).expect("complete native request"))
    );
    assert_eq!(fixture.counts().await, (1, 1, 1));
}

fn item(
    fixture: &Decisions<'_>,
    source: &Uuid7,
    range: Range<i32>,
) -> garde::Valid<DecisionReservationInput> {
    let mut input = fixture
        .reservation(
            DecisionAssessment::Item {
                ordinal: 0,
                interpretation_digest: "a".repeat(64),
                source_start: range.start,
                source_end: range.end,
            },
            None,
        )
        .into_inner();
    input.source_admission_id = source.to_string();
    garde::Unvalidated::new(input)
        .validate()
        .expect("bounded independent item request")
}

async fn source_snapshot(fixture: &Decisions<'_>, source: &Uuid7) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(a) FROM fleet.admissions a WHERE a.id=$1::uuid")
        .bind(source.as_str())
        .fetch_one(
            &mut *fixture
                .case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("native source observer"),
        )
        .await
        .expect("entire admitted source and claim state")
}
