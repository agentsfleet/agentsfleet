//! Bad semantic answers pay once; missing usage keeps credit until a real receipt.

use af_core::{error_code, id::Uuid7};
use af_wire::{
    decision::{DecisionAnswer, DecisionOutcome, DecisionResult},
    task::{
        DecisionCallState, DecisionNotSentCause, DecisionReceipt, DecisionReceiptInput,
        DecisionRecordDisposition, DecisionTaskVersion,
    },
};
use afd_fleet::task::TaskLifecycle as _;
use serde_json::json;

use super::{
    super::fixture::{Case, exercise, id},
    fixture::{Decisions, known, request},
    reply::known_result,
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn unusable_goal_results_preserve_authority_and_missing_usage_never_becomes_free() {
    exercise(unusable).await;
}

async fn unusable(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let initial = fixture
        .tasks
        .read(&case.scope, &id(&task.task_id))
        .await
        .expect("accepted goal");
    let balance = fixture.wallet().await;
    semantic(&fixture, &task, |r| r.result = DecisionOutcome::Refused).await;
    semantic(&fixture, &task, |r| r.reported_model = None).await;
    semantic(&fixture, &task, |r| {
        if let DecisionOutcome::Answered { answers } = &mut r.result {
            *answers
                .get_mut("goal.defect-fixed")
                .expect("accepted Noul question") = DecisionAnswer::Score {
                score: 0.0,
                confidence: Some(1.0),
                probabilities: [(0, 1.0)].into(),
            };
        }
    })
    .await;
    assert_eq!(fixture.wallet().await, balance);
    fixture.case.held.relay().await;
    assert_eq!(fixture.wallet().await, balance - 9);
    uncertain(&fixture, &task, |r| r.usage = None).await;
    uncertain(&fixture, &task, |r| {
        r.usage.as_mut().expect("fixture usage").cached_input_tokens = 1001;
    })
    .await;
    let mut expected = initial;
    expected.context.usage.model_nanos = 15;
    expected.context.usage.decision_calls = 5;
    expected.resource_version += 10;
    assert_eq!(
        fixture
            .tasks
            .read(&case.scope, &id(&task.task_id))
            .await
            .expect("accepted work remains running"),
        expected
    );
    assert_eq!(fixture.counts().await, (5, 5, 5));
    fixture.case.held.relay().await;
    assert_eq!(fixture.wallet().await, balance - 15);
    ledger(&fixture).await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn semantic(
    fixture: &Decisions<'_>,
    task: &DecisionTaskVersion,
    change: impl FnOnce(&mut DecisionResult),
) {
    let input = fixture.goal_reservation(task);
    let (_, body) = fixture.reserve(&input).await;
    let request = request(&body);
    let mut result = known_result(&request);
    change(&mut result);
    let receipt = DecisionReceipt::Provider {
        result: result.clone(),
    };
    let written = fixture.record(&request.call_id, receipt.clone()).await;
    assert_eq!(
        (
            written.state,
            written.disposition,
            written.charged_nanos.as_deref(),
            written.reserved_nanos.as_str()
        ),
        (
            DecisionCallState::Known,
            DecisionRecordDisposition::Unusable,
            Some("3"),
            "0"
        )
    );
    let repeated = fixture.record(&request.call_id, receipt).await;
    let mut expected = written;
    expected.replayed = true;
    assert_eq!(repeated, expected);
    let (issued, saved) = fixture.reserve(&input).await;
    assert!(!issued);
    assert_eq!(
        saved,
        json!({"status":"existing","call_id":request.call_id,
        "state":"known","recorded":{"request":request,"result":result,"disposition":"unusable"}})
    );
}

async fn uncertain(
    fixture: &Decisions<'_>,
    task: &DecisionTaskVersion,
    change: impl FnOnce(&mut DecisionResult),
) {
    let (_, body) = fixture.reserve(&fixture.goal_reservation(task)).await;
    let request = request(&body);
    let mut result = known_result(&request);
    change(&mut result);
    let receipt = DecisionReceipt::Provider { result };
    let before = fixture.counts().await;
    let balance = fixture.wallet().await;
    let written = fixture.record(&request.call_id, receipt.clone()).await;
    assert_eq!(
        (
            written.state,
            written.disposition,
            written.charged_nanos,
            written.usage,
            written.reserved_nanos.as_str()
        ),
        (
            DecisionCallState::Unknown,
            DecisionRecordDisposition::Unusable,
            None,
            None,
            "20000000"
        )
    );
    assert!(fixture.record(&request.call_id, receipt).await.replayed);
    assert_eq!(fixture.counts().await, before);
    assert_eq!(fixture.wallet().await, balance);
    refuse_unsent(fixture, &request.call_id).await;
    late_known(fixture, &request, before, balance).await;
}

async fn late_known(
    fixture: &Decisions<'_>,
    request: &af_wire::decision::DecisionRequest,
    before: (i64, i64, i64),
    balance: i64,
) {
    let valid = known(request);
    let late = fixture.record(&request.call_id, valid.clone()).await;
    assert_eq!(
        (
            late.state,
            late.disposition,
            late.charged_nanos.as_deref(),
            late.reserved_nanos.as_str()
        ),
        (
            DecisionCallState::Known,
            DecisionRecordDisposition::Current,
            Some("3"),
            "0"
        )
    );
    assert!(fixture.record(&request.call_id, valid).await.replayed);
    fixture.case.held.relay().await;
    assert_eq!(fixture.wallet().await, balance - 3);
    assert_eq!(fixture.counts().await, (before.0, before.1, before.2 + 1));
}

async fn refuse_unsent(fixture: &Decisions<'_>, call: &str) {
    let input = garde::Unvalidated::new(DecisionReceiptInput {
        fencing_token: fixture.case.lease().fencing_token,
        receipt: DecisionReceipt::NotSent {
            cause: DecisionNotSentCause::Build,
        },
    })
    .validate()
    .expect("bounded contradictory receipt");
    let fault = fixture
        .tasks
        .record_decision(fixture.case.lease(), &id(call), &input)
        .await
        .expect_err("provider data cannot later become unsent");
    assert_eq!(fault.code(), error_code::TASK_REPLAY_CONFLICT);
}

async fn ledger(fixture: &Decisions<'_>) {
    let lines: Vec<(Uuid7,String,String,Uuid7,i64)> = sqlx::query_as("SELECT workspace_id,provider,model,event_id,charged_nanos FROM billing.usage_lines WHERE work_id=$1 AND seq >= $2 ORDER BY seq")
        .bind(&fixture.case.held.lease_key).bind(i64::from(af_wire::usage::DECISION_SEQ_BASE))
        .fetch_all(&mut *fixture.case.held.fixtures.database.acquire().await.expect("original ledger observer")).await.expect("all five physical charges");
    let expected = (
        fixture.case.scope.workspace_id.clone(),
        "typesafe".to_owned(),
        fixture.model.model.model_name.clone(),
        fixture.case.held.event_id.clone(),
        3,
    );
    assert_eq!(lines, vec![expected; 5]);
}
