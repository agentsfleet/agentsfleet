//! A funding outage saves no call, and the reservation that follows recovery reads the
//! key as it then stands.

use std::sync::Arc;

use af_core::{error_code, test_util::trace::Capture};
use af_wire::durable_task::{
    DecisionCallState, DecisionRecordDisposition, DecisionReservationInput,
};
use afd_billing::{biller::FailOpen, ledger::LedgerBiller};
use afd_db::test_util::unreachable_db;
use afd_fleet::durable_task::{DurableTaskLifecycle as _, DurableTasks};
use afd_vault::{SecretBody, SecretName};
use serde_json::{Value, json, value::RawValue};

use super::{
    super::fixture::{Case, exercise},
    fixture::{Decisions, known, request},
    steps::another_operation,
};
use crate::seed::{MODEL, PROVIDER};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_funding_outage_saves_no_call_and_recovery_reads_a_fresh_key() {
    let capture = Capture::install();
    exercise(async |case| funding(case, &capture).await).await;
}

async fn funding(case: &Case, capture: &Capture) {
    let mut fixture = Decisions::create(case).await;
    let input = fixture.call_input().await;
    unpriced(&fixture, &input).await;
    unconfirmed(&fixture, &input).await;
    funding_events(capture, case);
    rotate_key(&fixture).await;
    let (issued, body) = fixture.reserve(&input).await;
    assert!(issued);
    let original = body
        .get("call_id")
        .and_then(Value::as_str)
        .expect("the issued call")
        .to_owned();
    assert_eq!(
        body.pointer("/dispatch/api_key"),
        Some(&json!("rotated-native-fixture"))
    );
    let independent = another_operation(&input);
    let (issued, replay) = fixture.reserve(&independent).await;
    assert!(!issued);
    assert_eq!(
        replay,
        json!({"status":"existing", "call_id":original, "state":"issued"})
    );
    assert_eq!(fixture.counts().await, (1, 1, 0));
    changed_evidence_conflicts(&fixture, &input).await;
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

async fn unpriced(fixture: &Decisions<'_>, input: &garde::Valid<DecisionReservationInput>) {
    let case = fixture.case;
    let removed = sqlx::query("DELETE FROM model.platform_model_prices WHERE decision_revision_id IS NULL AND platform_model_id IN (SELECT id FROM model.platform_models WHERE provider=$1 AND model_name=$2)")
        .bind(PROVIDER).bind(MODEL)
        .execute(&mut *case.held.fixtures.database.acquire().await.expect("owned rate injection"))
        .await.expect("remove this isolated fixture's coding price");
    assert_eq!(removed.rows_affected(), 1);
    let independent = another_operation(input);
    for candidate in [input, &independent] {
        let failure = fixture
            .tasks
            .reserve_decision(case.run(), candidate)
            .await
            .expect_err("decision issuance requires known whole-work credit headroom");
        assert_eq!(
            failure.code(),
            error_code::DURABLE_TASK_DECISION_UNAVAILABLE
        );
        assert_eq!(fixture.counts().await, (0, 0, 0), "a refusal saves nothing");
    }
    case.held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
}

/// The same assessment from another operation with other evidence conflicts with the
/// issued call and changes nothing.
async fn changed_evidence_conflicts(
    fixture: &Decisions<'_>,
    input: &garde::Valid<DecisionReservationInput>,
) {
    let saved = fixture.call_snapshot().await;
    let mut changed = (*another_operation(input)).clone();
    changed.evidence = json!({"observation":"a different tested head"});
    let changed = garde::Unvalidated::new(changed)
        .validate()
        .expect("bounded conflicting assessment");
    let failure = fixture
        .tasks
        .reserve_decision(fixture.case.run(), &changed)
        .await
        .expect_err("another operation cannot change an issued assessment");
    assert_eq!(failure.code(), error_code::DURABLE_TASK_REPLAY_CONFLICT);
    assert_eq!(fixture.call_snapshot().await, saved);
    assert_eq!(fixture.counts().await, (1, 1, 0));
}

fn funding_events(capture: &Capture, case: &Case) {
    let events = capture.events();
    let reasons = events
        .iter()
        .filter(|event| event.field("event") == Some("dt_decision_funding_refused"))
        .map(|event| (event.field("reason"), event.field("run_id")))
        .collect::<Vec<_>>();
    assert_eq!(
        reasons,
        [
            (
                Some("coding_price_unavailable"),
                Some(case.run().run_id.as_str())
            ),
            (
                Some("coding_price_unavailable"),
                Some(case.run().run_id.as_str())
            ),
            (Some("credit_unconfirmed"), Some(case.run().run_id.as_str())),
        ]
    );
}

async fn unconfirmed(fixture: &Decisions<'_>, input: &garde::Valid<DecisionReservationInput>) {
    let case = fixture.case;
    let mut dependencies = fixture.dependencies.clone();
    dependencies.biller = Arc::new(FailOpen::new(LedgerBiller::new(unreachable_db())));
    let unconfirmed = DurableTasks::new(case.held.runs.clone(), case.turns.clone(), dependencies);
    let failure = unconfirmed
        .reserve_decision(case.run(), input)
        .await
        .expect_err("a physical decision needs confirmed funding");
    assert_eq!(
        failure.code(),
        error_code::DURABLE_TASK_DECISION_UNAVAILABLE
    );
    assert_eq!(fixture.counts().await, (0, 0, 0), "a refusal saves nothing");
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
