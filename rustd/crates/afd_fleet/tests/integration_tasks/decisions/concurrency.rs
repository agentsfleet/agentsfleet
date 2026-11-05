//! Concurrent replays and independent operations share one grant and charge.

use std::sync::atomic::{AtomicUsize, Ordering};

use afd_core::error_code;
use afd_fleet::task::TaskLifecycle as _;
use afd_wire::{
    task::{DecisionAssessment, DecisionCallState, DecisionRecordDisposition},
    usage::DECISION_SEQ_BASE,
};
use futures_util::future::join_all;
use serde_json::{Value, json};
use tokio::sync::Barrier;

use super::{
    super::fixture::{Case, exercise},
    fixture::{Decisions, known, request},
};

#[derive(Clone, Copy)]
enum Operations {
    Same,
    Independent,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn concurrent_decision_replays_issue_one_key_and_account_one_complete_line() {
    for operations in [Operations::Same, Operations::Independent] {
        exercise(async |case| replays(case, operations).await).await;
    }
}

async fn replays(case: &Case, operations: Operations) {
    let fixture = Decisions::create(case).await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let replies = contend(&fixture, &input, operations).await;
    assert_eq!(replies.iter().filter(|(issued, _)| *issued).count(), 1);
    let body = &replies
        .iter()
        .find(|(issued, _)| *issued)
        .expect("one physical send")
        .1;
    let call = body
        .get("call_id")
        .and_then(Value::as_str)
        .expect("native call");
    assert!(
        replies
            .iter()
            .all(|(_, body)| body.get("call_id") == Some(&json!(call)))
    );
    let expected = json!({"status":"existing", "call_id":call, "state":"issued"});
    for (_, body) in replies.iter().filter(|(issued, _)| !issued) {
        assert_eq!(body, &expected);
    }
    assert_eq!(fixture.counts().await, (1, 1, 0));
    let request = compiled_request(&fixture, body);
    accounted(&fixture, call, &request).await;
    saved_replay(&fixture, &input, &request).await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

fn compiled_request(fixture: &Decisions<'_>, body: &Value) -> afd_wire::decision::DecisionRequest {
    assert_eq!(
        body.pointer("/dispatch/assignment"),
        Some(&serde_json::to_value(&fixture.assignment).expect("frozen assignment"))
    );
    let request = request(body);
    assert_eq!(
        request
            .questions
            .iter()
            .map(|question| question.name.as_str())
            .collect::<Vec<_>>(),
        ["intent", "association", "multiple_requests"]
    );
    assert_eq!(
        request.state.get("message"),
        Some(&json!(super::super::fixture::GOALS.join("; ")))
    );
    assert!(
        request.state.get("assessed_message").is_none(),
        "whole messages are not copied twice"
    );
    request
}

async fn contend(
    fixture: &Decisions<'_>,
    input: &garde::Valid<afd_wire::task::DecisionReservationInput>,
    operations: Operations,
) -> Vec<(bool, Value)> {
    const CALLERS: usize = 100;
    let barrier = Barrier::new(CALLERS);
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let peers = match operations {
        Operations::Same => Vec::new(),
        Operations::Independent => (1..CALLERS)
            .map(|_| fixture.reservation(DecisionAssessment::Message, None))
            .collect::<Vec<_>>(),
    };
    let requests = match operations {
        Operations::Same => vec![input; CALLERS],
        Operations::Independent => std::iter::once(input).chain(peers.iter()).collect(),
    };
    let replies = join_all(requests.into_iter().map(|input| {
        let (barrier, active, peak) = (&barrier, &active, &peak);
        async move {
            barrier.wait().await;
            peak.fetch_max(
                active.fetch_add(1, Ordering::Relaxed) + 1,
                Ordering::Relaxed,
            );
            let _active = scopeguard::guard((), |()| {
                active.fetch_sub(1, Ordering::Relaxed);
            });
            fixture.reserve(input).await
        }
    }))
    .await;
    assert!(peak.load(Ordering::Relaxed) > 1);
    assert_eq!(active.load(Ordering::Relaxed), 0);
    replies
}

async fn accounted(
    fixture: &Decisions<'_>,
    call: &str,
    request: &afd_wire::decision::DecisionRequest,
) {
    let receipt = known(request);
    let recorded = fixture.record(call, receipt.clone()).await;
    assert_eq!(
        (
            recorded.state,
            recorded.replayed,
            recorded.disposition,
            recorded.charged_nanos.as_deref(),
            recorded.overage_nanos.as_deref(),
            recorded.reserved_nanos.as_str()
        ),
        (
            DecisionCallState::Known,
            false,
            DecisionRecordDisposition::Current,
            Some("3"),
            Some("0"),
            "0"
        )
    );
    let mut expected_replay = recorded;
    expected_replay.replayed = true;
    assert_eq!(fixture.record(call, receipt).await, expected_replay);
    assert_eq!(fixture.counts().await, (1, 1, 1));
    assert_line(fixture).await;
}

async fn saved_replay(
    fixture: &Decisions<'_>,
    input: &garde::Valid<afd_wire::task::DecisionReservationInput>,
    request: &afd_wire::decision::DecisionRequest,
) {
    let (issued, replay) = fixture.reserve(input).await;
    assert!(!issued);
    assert_eq!(
        replay.pointer("/recorded/request"),
        Some(&serde_json::to_value(request).expect("request JSON"))
    );
    assert_eq!(
        replay.pointer("/recorded/disposition"),
        Some(&json!("current"))
    );
    assert!(!replay.to_string().contains("native-decision-fixture-"));
    let mut changed = (**input).clone();
    changed.evidence = json!({"different":"meaning"});
    let changed = garde::Unvalidated::new(changed)
        .validate()
        .expect("bounded changed operation");
    let fault = fixture
        .tasks
        .reserve_decision(fixture.case.lease(), &changed)
        .await
        .expect_err("operation meaning is immutable");
    assert_eq!(fault.code(), error_code::TASK_REPLAY_CONFLICT);
    assert_eq!(fixture.counts().await, (1, 1, 1));
}

async fn assert_line(fixture: &Decisions<'_>) {
    let row: (i64, i64, i64, i64, i64, i64, String, String, String) = sqlx::query_as(
        "SELECT seq,input_token_count,cached_input_token_count,output_token_count,runtime_ms,charged_nanos,provider,model,tenant_id::text FROM billing.usage_lines WHERE work_id=$1::uuid AND seq >= $2")
        .bind(fixture.case.held.lease_key.as_str()).bind(i64::from(DECISION_SEQ_BASE))
        .fetch_one(&mut *fixture.case.held.fixtures.database.acquire().await.expect("original work ledger"))
        .await.expect("one complete charge line");
    assert_eq!(
        row,
        (
            i64::from(DECISION_SEQ_BASE),
            800,
            200,
            100,
            0,
            3,
            "typesafe".to_owned(),
            fixture.model.model.model_name.clone(),
            fixture.case.held.tenant.clone()
        )
    );
}
