//! Erasure removes private answers while preserving original payer accounting.

use af_wire::task::{DecisionAssessment, DecisionCallState, DecisionRecordDisposition};
use serde_json::Value;

use super::{
    super::fixture::{Case, exercise},
    fixture::{Decisions, known, request},
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn goal_usage_survives_fleet_erasure_without_recreating_private_payloads() {
    exercise(erased).await;
}

async fn erased(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let input = fixture.reservation(
        DecisionAssessment::Goal {
            action_group_id: afd_db::test_util::mint_id().into(),
        },
        Some(task),
    );
    let (_, body) = fixture.reserve(&input).await;
    let request = request(&body);
    assert_eq!(
        request
            .questions
            .iter()
            .map(|question| question.name.as_str())
            .collect::<Vec<_>>(),
        [
            "goal.defect-fixed",
            "goal.verification-current",
            "next_action"
        ]
    );
    let call = body
        .get("call_id")
        .and_then(Value::as_str)
        .expect("native call");
    sqlx::query("DELETE FROM fleet.fleets WHERE id=$1")
        .bind(&case.held.fleet)
        .execute(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("owner deletion connection"),
        )
        .await
        .expect("ordinary fleet deletion cascades private task data");
    assert_eq!(fixture.counts().await, (1, 0, 0));
    late_accounting(&fixture, call, &request, &input).await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn late_accounting(
    fixture: &Decisions<'_>,
    call: &str,
    request: &af_wire::decision::DecisionRequest,
    input: &garde::Valid<af_wire::task::DecisionReservationInput>,
) {
    let receipt = known(request);
    let result = fixture.record(call, receipt.clone()).await;
    assert_eq!(
        (
            result.state,
            result.disposition,
            result.charged_nanos.as_deref(),
            result.reserved_nanos.as_str()
        ),
        (
            DecisionCallState::Known,
            DecisionRecordDisposition::Unusable,
            Some("3"),
            "0"
        )
    );
    assert!(fixture.record(call, receipt).await.replayed);
    assert_eq!(fixture.counts().await, (1, 0, 1));
    assert_eq!(fixture.case.counts().await, (0, 0, 0));
    let (issued, replay) = fixture.reserve(input).await;
    assert!(!issued);
    assert!(
        replay.get("recorded").is_none(),
        "late replay cannot recreate erased evidence"
    );
    assert!(!replay.to_string().contains("native-decision-fixture-"));
}
