//! An already admitted identity survives a marker change; new work needs review.

use super::{Fleet, Method, StatusCode, json, json_body, mint, send};

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_policy_marker_change_refuses_new_admissions_but_preserves_accepted_replay() {
    let logs = af_core::test_util::trace::Capture::install();
    let fleet = Fleet::create().await;
    let key = mint();
    let body = json!({"message":"Check the latest evidence","operation_id":key});
    let messages = format!("{}/messages", fleet.item);
    let first = send(
        &fleet.case.router,
        Method::POST,
        &messages,
        Some(&fleet.case.base.token),
        &body.to_string(),
    )
    .await;
    let status = first.status();
    let first = json_body(first).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{first}");
    sqlx::query("WITH incompatible AS (INSERT INTO fleet.revisions(id,fleet_id,revision,definition,canonical_digest,policy_semantics,created_at,runtime_model_ref)
SELECT $2,fleet_id,revision+1,definition,canonical_digest,'policy-unavailable-fixture',created_at,runtime_model_ref
FROM fleet.revisions WHERE fleet_id=$1 RETURNING id)
UPDATE fleet.fleets SET head_revision_id=(SELECT id FROM incompatible) WHERE id=$1")
        .bind(crate::authoring::id_field(&fleet.saved["id"])).bind(mint())
        .execute(&mut *fleet.case.base.db.acquire().await.expect("policy connection")).await.expect("change marker");
    let repeated = send(
        &fleet.case.router,
        Method::POST,
        &messages,
        Some(&fleet.case.base.token),
        &body.to_string(),
    )
    .await;
    let status = repeated.status();
    let repeated = json_body(repeated).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "{repeated} {:?}",
        logs.events()
    );
    assert_eq!(repeated["event_id"], first["event_id"]);
    let new = json!({"message":"New work after the policy change","operation_id":mint()});
    let refused = send(
        &fleet.case.router,
        Method::POST,
        &messages,
        Some(&fleet.case.base.token),
        &new.to_string(),
    )
    .await;
    assert_eq!(
        refused.status(),
        StatusCode::CONFLICT,
        "{}",
        json_body(refused).await
    );
    fleet.case.cleanup().await;
}
