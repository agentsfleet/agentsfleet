//! Mutation bodies retain the original native identities across HTTP serialization.
use super::*;

fn binding() -> Value {
    json!({"fencing_token":FENCE,"source_admission_id":ID,"accepted_revision_id":ID,
        "plan_digest":"a".repeat(64),"message_decision_call_id":EFFECT,
        "items":[{"proposed":{"source_start":0,"source_end":5,"action":"conversation","task_id":null,"title":"Reply","interpretation":"Answer the question","target":null},"decision_call_id":null}]})
}
fn receipt() -> Value {
    json!({"fencing_token":FENCE,"operation_id":EFFECT,"source_admission_id":ID,
        "plan_digest":"a".repeat(64),"item_id":ID,"task":{"task_id":ID,"generation":7},
        "mode":"answered","response_text":"Private source answer"})
}
fn publication() -> Value {
    json!({"operation_id":EFFECT,"fencing_token":FENCE,"task":{"task_id":ID,"generation":7},
        "source_item_id":ID,"covered_input_sequence":9,"expected_head":"a".repeat(40),
        "tested_tree":"b".repeat(40),"changes":[{"path":"src/fix.rs","contents":"Zml4"}]})
}
#[tokio::test]
async fn typed_bindings_and_receipts_preserve_complete_native_request_and_replay() {
    let bound = json!({"items":[],"current_task":null,"replayed":true});
    let answered =
        json!({"item_id":ID,"mode":"answered","covered_input_sequence":9,"replayed":true});
    let server = Server::start(vec![
        request(Method::POST, "bindings", binding(), bound.clone()),
        request(Method::POST, "input-receipts", receipt(), answered.clone()),
    ])
    .await;
    let lease = Uuid7::parse(ID).unwrap();
    let result = server
        .plane
        .task_bindings(&lease, &serde_json::from_value(binding()).unwrap())
        .await
        .unwrap();
    assert_eq!(serde_json::to_value(result).unwrap(), bound);
    let result = server
        .plane
        .task_input_receipt(&lease, &serde_json::from_value(receipt()).unwrap())
        .await
        .unwrap();
    assert_eq!(serde_json::to_value(result).unwrap(), answered);
    server.finish();
}
#[tokio::test]
async fn typed_publication_retains_tested_tree_and_operation_identity() {
    let server = Server::start(vec![request(
        Method::POST,
        "effects",
        publication(),
        effect(),
    )])
    .await;
    let result = server
        .plane
        .task_publish(
            &Uuid7::parse(ID).unwrap(),
            &serde_json::from_value(publication()).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(serde_json::to_value(result).unwrap(), effect());
    server.finish();
}

#[tokio::test]
async fn a_native_publication_refusal_is_not_redispatched_or_reinterpreted() {
    let mut refused = request(
        Method::POST,
        "effects",
        publication(),
        json!({"error_code":"UZ-RUN-011"}),
    );
    refused.status = StatusCode::CONFLICT;
    let server = Server::start(vec![refused]).await;
    let failure = server
        .plane
        .task_publish(
            &Uuid7::parse(ID).unwrap(),
            &serde_json::from_value(publication()).unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(failure.refusal_status(), Some(409));
    assert_eq!(
        failure.refusal_code(),
        Some(afd_core::error_code::RUN_LEASE_LOST)
    );
    server.finish();
}
