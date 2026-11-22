//! Task verbs refuse well-formed JSON that breaks a declared bound before any store.
//!
//! Every body and query here deserialises; only its `garde` bound is wrong. A
//! bound that stopped being checked would send the request on to the absent
//! datastore, which answers 503 rather than the 400 asserted here, so each
//! refusal also carries its admitted twin.

use afd_auth::directory::Liveness;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::{FLEET_ID, RUN_ID, RUNNER_TOKEN, code_of};
use crate::harness::{Fleet, runner_id, send};

/// A second `UUIDv7`, for the identifiers a body names beside the run.
const ITEM_ID: &str = "01924f4e-0000-7000-8000-00000000b0b0";

fn tasks(resource: &str) -> String {
    format!("/v1/runners/me/runs/{RUN_ID}/durable_tasks/{resource}")
}

fn router() -> axum::Router {
    Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router()
}

/// The out-of-bounds request is refused as malformed, and its in-bounds twin is
/// admitted to the store, which the suite leaves unreachable.
async fn bounded(
    router: &axum::Router,
    method: Method,
    refused: (&str, &Value),
    admitted: (&str, &Value),
) {
    let body = |value: &Value| {
        if value.is_null() {
            String::new()
        } else {
            value.to_string()
        }
    };
    let response = send(
        router,
        method.clone(),
        refused.0,
        Some(RUNNER_TOKEN),
        &body(refused.1),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "{} {}",
        refused.0,
        refused.1
    );
    assert_eq!(code_of(response).await, "UZ-REQ-001", "{}", refused.0);
    let response = send(
        router,
        method,
        admitted.0,
        Some(RUNNER_TOKEN),
        &body(admitted.1),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "{} {}",
        admitted.0,
        admitted.1
    );
    assert_eq!(code_of(response).await, "UZ-INTERNAL-001", "{}", admitted.0);
}

#[tokio::test]
async fn a_publication_with_no_change_is_refused_before_the_durable_task_store() {
    let router = router();
    let route = tasks("effects");
    let publication = |changes: Value| {
        json!({"operation_id": ITEM_ID, "fencing_token": 7,
            "durable_task": {"durable_task_id": FLEET_ID, "generation": 1}, "source_item_id": ITEM_ID,
            "covered_input_sequence": 0, "expected_head": "0".repeat(40),
            "tested_tree": "1".repeat(40), "changes": changes})
    };
    bounded(
        &router,
        Method::POST,
        (&route, &publication(json!([]))),
        (
            &route,
            &publication(json!([{"path": "README.md", "contents": "aGk="}])),
        ),
    )
    .await;
}

#[tokio::test]
async fn a_publication_page_for_a_short_durable_task_id_or_no_rows_is_refused() {
    let router = router();
    let route = tasks("effects");
    let admitted = format!("{route}?fencing_token=7&durable_task_id={FLEET_ID}");
    for refused in [
        format!("{route}?fencing_token=7&durable_task_id=short"),
        format!("{route}?fencing_token=7&durable_task_id={FLEET_ID}&limit=0"),
    ] {
        bounded(
            &router,
            Method::GET,
            (&refused, &Value::Null),
            (&admitted, &Value::Null),
        )
        .await;
    }
}

#[tokio::test]
async fn a_decision_receipt_with_a_status_outside_http_is_refused() {
    let router = router();
    let route = tasks(&format!("decisions/{ITEM_ID}"));
    let receipt = |status: u16| {
        json!({"fencing_token": 7, "receipt": {"outcome": "unknown", "cause": {
            "reason": "provider_status", "status": status,
            "retry_delay": {"kind": "unspecified"}}}})
    };
    for status in [99, 600] {
        bounded(
            &router,
            Method::PATCH,
            (&route, &receipt(status)),
            (&route, &receipt(429)),
        )
        .await;
    }
}

#[tokio::test]
async fn an_interpretation_with_no_items_is_refused() {
    let router = router();
    let route = tasks("bindings");
    let response = send(
        &router,
        Method::POST,
        &route,
        Some(RUNNER_TOKEN),
        &json!({"fencing_token": 7, "source_turn_id": ITEM_ID,
            "accepted_revision_id": ITEM_ID, "plan_digest": "a".repeat(64),
            "message_decision_call_id": ITEM_ID, "items": []})
        .to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(response).await, "UZ-REQ-001");
}

#[tokio::test]
async fn an_input_receipt_naming_a_non_uuid_operation_is_refused() {
    let router = router();
    let route = tasks("input-receipts");
    let receipt = |operation: &str| {
        json!({"fencing_token": 7, "operation_id": operation, "source_turn_id": ITEM_ID,
            "plan_digest": "a".repeat(64), "item_id": ITEM_ID, "mode": "queued"})
    };
    for operation in ["not-a-uuid", "01924F4E-0000-6000-8000-00000000B0B0", "é"] {
        bounded(
            &router,
            Method::POST,
            (&route, &receipt(operation)),
            (&route, &receipt(ITEM_ID)),
        )
        .await;
    }
}

#[tokio::test]
async fn a_checkpoint_read_under_a_malformed_pin_is_refused() {
    let router = router();
    let checkpoint = tasks(&format!("checkpoints/{ITEM_ID}"));
    for resource in [
        format!("{checkpoint}/objects"),
        format!("{checkpoint}/pages/{FLEET_ID}"),
    ] {
        let admitted = format!("{resource}?fencing_token=7&pin_id={ITEM_ID}");
        for pin in ["short", "01924F4E-0000-7000-8000-00000000B0B0"] {
            let refused = format!("{resource}?fencing_token=7&pin_id={pin}");
            bounded(
                &router,
                Method::GET,
                (&refused, &Value::Null),
                (&admitted, &Value::Null),
            )
            .await;
        }
    }
}
