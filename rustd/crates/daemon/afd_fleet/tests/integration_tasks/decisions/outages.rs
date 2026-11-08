//! Funding outages preserve one unsent call and resolve fresh keys on recovery.

use std::sync::Arc;

use af_core::{error_code, test_util::trace::Capture};
use af_wire::task::{
    DecisionAssessment, DecisionCallState, DecisionReceipt, DecisionReceiptInput,
    DecisionRecordDisposition, DecisionReservationInput, DecisionUnavailableCause,
};
use afd_billing::{
    biller::{OutagePolicy, WithOutagePolicy},
    ledger::LedgerBiller,
};
use afd_db::test_util::unreachable_db;
use afd_fleet::task::{TaskLifecycle as _, Tasks};
use afd_vault::{SecretBody, SecretName};
use serde_json::{Value, json, value::RawValue};

use super::{
    super::fixture::{Case, exercise, id},
    fixture::{Decisions, known, request},
};
use crate::seed::{MODEL, PROVIDER};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn unconfirmed_credit_keeps_the_intent_and_resolves_a_fresh_key_on_recovery() {
    let capture = Capture::install();
    exercise(async |case| funding(case, &capture).await).await;
}

async fn funding(case: &Case, capture: &Capture) {
    let mut fixture = Decisions::create(case).await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let original = unpriced(&fixture, &input).await;
    unconfirmed(&fixture, &input).await;
    funding_events(capture, case);
    rotate_key(&fixture).await;
    let (issued, body) = fixture.reserve(&input).await;
    assert!(issued);
    assert_eq!(body.get("call_id"), Some(&json!(original)));
    assert_eq!(
        body.pointer("/dispatch/api_key"),
        Some(&json!("rotated-native-fixture"))
    );
    let independent = fixture.reservation(DecisionAssessment::Message, None);
    let (issued, replay) = fixture.reserve(&independent).await;
    assert!(!issued);
    assert_eq!(
        replay,
        json!({"status":"existing", "call_id":original, "state":"issued"})
    );
    assert_eq!(fixture.counts().await, (1, 1, 0));
    disabled_after_issue(&mut fixture, &input, &body).await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn rotate_key(fixture: &Decisions<'_>) {
    let raw = RawValue::from_string(json!({"api_key":"rotated-native-fixture"}).to_string())
        .expect("synthetic rotated key");
    fixture
        .vault
        .replace(
            &fixture.case.scope.workspace_id,
            &SecretName::parse("typesafe").expect("fixed provider bag"),
            &SecretBody::parse(&raw).expect("valid key object"),
        )
        .await
        .expect("admin rotates before first issuance");
}

async fn unpriced(
    fixture: &Decisions<'_>,
    input: &garde::Valid<DecisionReservationInput>,
) -> String {
    let case = fixture.case;
    let removed = sqlx::query("DELETE FROM model.platform_model_prices WHERE platform_model_id IN (SELECT id FROM model.platform_models WHERE provider=$1 AND model_name=$2)")
        .bind(PROVIDER).bind(MODEL)
        .execute(&mut *case.held.fixtures.database.acquire().await.expect("owned rate injection"))
        .await.expect("remove this isolated fixture's coding price");
    assert_eq!(removed.rows_affected(), 1);
    let failure = fixture
        .tasks
        .reserve_decision(case.lease(), input)
        .await
        .expect_err("decision issuance requires known whole-work credit headroom");
    assert_eq!(failure.code(), error_code::TASK_DECISION_UNAVAILABLE);
    assert_eq!(fixture.counts().await, (1, 1, 0));
    let call = unissued_replays(fixture).await;
    case.held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
    call
}

async fn unissued_replays(fixture: &Decisions<'_>) -> String {
    let saved = fixture.call_snapshot().await;
    let call = saved
        .0
        .get("id")
        .and_then(Value::as_str)
        .expect("retained native call");
    let independent = fixture.reservation(DecisionAssessment::Message, None);
    let mut changed = (*independent).clone();
    changed.operation_id = afd_db::test_util::mint_id();
    changed.evidence = json!({"observation":"a different tested head"});
    let changed = garde::Unvalidated::new(changed)
        .validate()
        .expect("bounded conflicting assessment");
    for (candidate, expected) in [
        (&independent, error_code::TASK_DECISION_UNAVAILABLE),
        (&changed, error_code::TASK_REPLAY_CONFLICT),
    ] {
        let failure = fixture
            .tasks
            .reserve_decision(fixture.case.lease(), candidate)
            .await
            .expect_err(
                "independent operations cannot duplicate or change an unfinished assessment",
            );
        assert_eq!(failure.code(), expected);
        assert_eq!(fixture.call_snapshot().await, saved);
        assert_eq!(fixture.counts().await, (1, 1, 0));
    }
    premature_receipt(fixture, call).await;
    assert_eq!(fixture.call_snapshot().await, saved);
    assert_eq!(fixture.counts().await, (1, 1, 0));
    call.to_owned()
}

async fn premature_receipt(fixture: &Decisions<'_>, call: &str) {
    let receipt = garde::Unvalidated::new(DecisionReceiptInput {
        fencing_token: fixture.case.lease().fencing_token,
        receipt: DecisionReceipt::Unknown {
            cause: DecisionUnavailableCause::Timeout,
        },
    })
    .validate()
    .expect("bounded premature receipt");
    let failure = fixture
        .tasks
        .record_decision(fixture.case.lease(), &id(call), &receipt)
        .await
        .expect_err("an unissued call cannot acquire a physical receipt");
    assert_eq!(failure.code(), error_code::TASK_INVALID_TARGET);
}

fn funding_events(capture: &Capture, case: &Case) {
    let events = capture.events();
    let reasons = events
        .iter()
        .filter(|event| event.field("event") == Some("dt_decision_funding_refused"))
        .map(|event| (event.field("reason"), event.field("lease_id")))
        .collect::<Vec<_>>();
    assert_eq!(
        reasons,
        [
            (
                Some("coding_price_unavailable"),
                Some(case.lease().lease_id.as_str())
            ),
            (
                Some("coding_price_unavailable"),
                Some(case.lease().lease_id.as_str())
            ),
            (
                Some("credit_unconfirmed"),
                Some(case.lease().lease_id.as_str())
            ),
        ]
    );
}

async fn unconfirmed(fixture: &Decisions<'_>, input: &garde::Valid<DecisionReservationInput>) {
    let case = fixture.case;
    let mut dependencies = fixture.dependencies.clone();
    dependencies.biller = Arc::new(WithOutagePolicy::new(
        LedgerBiller::on_system_clock(unreachable_db()),
        OutagePolicy::Open,
    ));
    let unconfirmed = Tasks::new(
        case.held.leases.clone(),
        case.admissions.clone(),
        dependencies,
    );
    let failure = unconfirmed
        .reserve_decision(case.lease(), input)
        .await
        .expect_err("a physical decision needs confirmed funding");
    assert_eq!(failure.code(), error_code::TASK_DECISION_UNAVAILABLE);
    assert_eq!(fixture.counts().await, (1, 1, 0));
}

async fn disabled_after_issue(
    fixture: &mut Decisions<'_>,
    input: &garde::Valid<DecisionReservationInput>,
    body: &Value,
) {
    fixture
        .model
        .disable()
        .await
        .expect("decision model fixture lifecycle");
    let call = body
        .get("call_id")
        .and_then(Value::as_str)
        .expect("one original call");
    let result = fixture.record(call, known(&request(body))).await;
    assert_eq!(
        (
            result.state,
            result.disposition,
            result.charged_nanos.as_deref()
        ),
        (
            DecisionCallState::Known,
            DecisionRecordDisposition::Stale,
            Some("3")
        )
    );
    assert_eq!(fixture.counts().await, (1, 1, 1));
    let (issued, saved) = fixture.reserve(input).await;
    assert!(!issued);
    assert_eq!(
        saved.pointer("/recorded/disposition"),
        Some(&json!("stale"))
    );
    assert!(!saved.to_string().contains("rotated-native-fixture"));
}
