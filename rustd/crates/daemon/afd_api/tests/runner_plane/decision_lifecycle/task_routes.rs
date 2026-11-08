//! Served task acceptance retains the paid proof and immutable recovery receipts.
#![expect(
    clippy::panic,
    reason = "synthetic decision question drift must fail the fixture"
)]

mod binding;
mod checkpoints;
mod errors;

use af_core::etag::sha256_hex;
use af_wire::{
    decision::{DecisionAnswer, DecisionOutcome},
    task::{
        DecisionReceipt, DecisionReceiptInput, TaskBindingItem, TaskBindingsInput, TaskInputAction,
        TaskProposedItem, TaskRequestedTarget,
    },
};
use afd_db::test_util::mint_id;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::{Fixture, assertions, setup, success};
use crate::harness::{json_body, send};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn accepted_task_http_recovers_without_reclassifying_or_reapplying() {
    let fixture = Fixture::create().await;
    let (binding, accepted) = binding::bind(&fixture).await;
    errors::refuse_malformed_requests(&fixture).await;
    let task = json!({"task_id":accepted.pointer("/current_task/task_id").expect("response field exists"),
        "generation":accepted.pointer("/current_task/generation").expect("response field exists")});
    receipts(&fixture, &binding, &accepted, &task).await;
    checkpoints::exercise(&fixture, &task).await;
    effects(
        &fixture,
        &task,
        accepted
            .pointer("/items/0/item_id")
            .expect("response field exists"),
    )
    .await;
    fixture.finish().await;
}

async fn effects(fixture: &Fixture, task: &Value, item: &Value) {
    let route = path(fixture, "effects");
    let query = format!(
        "{route}?fencing_token={}&task_id={}",
        fixture.lease.fence,
        task.pointer("/task_id")
            .expect("response field exists")
            .as_str()
            .expect("bound task")
    );
    let empty = success(fixture, Method::GET, &query, "", StatusCode::OK).await;
    assert_eq!(empty, json!({"effects":[],"next_cursor":null}));
    let missing = format!(
        "{route}/{}?fencing_token={}",
        mint_id(),
        fixture.lease.fence
    );
    let response = send(
        &fixture.router,
        Method::GET,
        &missing,
        Some(&fixture.runner.token),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    problem(&json_body(response).await, "UZ-DT-001");
    let publish = json!({"operation_id":mint_id(),"fencing_token":fixture.lease.fence,
        "task":task,"source_item_id":item,"covered_input_sequence":0,
        "expected_head":"0".repeat(40),"tested_tree":"1".repeat(40),
        "changes":[{"path":"README.md","contents":"aGk="}]});
    // An API-only task has no native GitHub publication authority.
    let refused = call(
        fixture,
        Method::POST,
        &route,
        &publish,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(
        refused
            .pointer("/error_code")
            .expect("response field exists"),
        "UZ-DT-004"
    );
    let unchanged = success(fixture, Method::GET, &query, "", StatusCode::OK).await;
    assert_eq!(unchanged, empty, "refused target allocates no publication");
}

fn path(fixture: &Fixture, resource: &str) -> String {
    format!(
        "/v1/runners/me/leases/{}/tasks/{resource}",
        fixture.lease.id
    )
}

async fn call(
    fixture: &Fixture,
    method: Method,
    route: &str,
    body: &Value,
    status: StatusCode,
) -> Value {
    let response = send(
        &fixture.router,
        method,
        route,
        Some(&fixture.runner.token),
        &body.to_string(),
    )
    .await;
    let actual = response.status();
    if actual.is_success() {
        assert_eq!(
            response
                .headers()
                .get(http::header::CACHE_CONTROL)
                .expect("sensitive task responses prohibit caching"),
            "no-store"
        );
    }
    let value = json_body(response).await;
    assert_eq!(actual, status, "{route}: {value}");
    value
}

async fn foreign_and_stale(fixture: &Fixture, route: &str, body: &Value) {
    for token in [None, Some(fixture.other.token.as_str())] {
        let response = send(
            &fixture.router,
            Method::POST,
            route,
            token,
            &body.to_string(),
        )
        .await;
        assert_eq!(
            response.status(),
            if token.is_none() {
                StatusCode::UNAUTHORIZED
            } else {
                // Native ownership lookup hides leases belonging to another runner.
                StatusCode::NOT_FOUND
            }
        );
        problem(
            &json_body(response).await,
            if token.is_none() {
                "UZ-RUN-001"
            } else {
                "UZ-RUN-006"
            },
        );
    }
    let mut stale = body.clone();
    *stale
        .get_mut("fencing_token")
        .expect("fixture field exists") = json!(fixture.lease.fence + 1);
    let refused = call(fixture, Method::POST, route, &stale, StatusCode::CONFLICT).await;
    problem(&refused, "UZ-RUN-005");
}

async fn receipts(fixture: &Fixture, binding: &TaskBindingsInput, accepted: &Value, task: &Value) {
    let input_path = path(fixture, "inputs");
    let inputs = success(
        fixture,
        Method::GET,
        &format!("{input_path}?fencing_token={}", fixture.lease.fence),
        "",
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        inputs
            .pointer("/current_task/task_id")
            .expect("response field exists"),
        task.pointer("/task_id").expect("response field exists")
    );
    assert_eq!(
        inputs
            .pointer("/current_interpretation/plan_digest")
            .expect("response field exists"),
        &binding.plan_digest
    );
    let receipt = json!({"fencing_token":fixture.lease.fence,"operation_id":mint_id(),
        "source_admission_id":fixture.lease.source,"plan_digest":binding.plan_digest,
        "item_id":accepted.pointer("/items/0/item_id").expect("response field exists"),"task":task,
        "mode":"answered","response_text":"I will inspect the failed checks."});
    acknowledge(fixture, &receipt).await;
}

async fn acknowledge(fixture: &Fixture, receipt: &Value) {
    let receipt_path = path(fixture, "input-receipts");
    let first = call(
        fixture,
        Method::POST,
        &receipt_path,
        receipt,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(
        first.pointer("/replayed").expect("response field exists"),
        false
    );
    let replay = call(
        fixture,
        Method::POST,
        &receipt_path,
        receipt,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        replay.pointer("/replayed").expect("response field exists"),
        true
    );
    assert_eq!(
        first
            .pointer("/covered_input_sequence")
            .expect("response field exists"),
        replay
            .pointer("/covered_input_sequence")
            .expect("response field exists")
    );
    let mut changed = receipt.clone();
    *changed
        .get_mut("response_text")
        .expect("fixture field exists") = json!("A conflicting second answer");
    let refused = call(
        fixture,
        Method::POST,
        &receipt_path,
        &changed,
        StatusCode::CONFLICT,
    )
    .await;
    problem(&refused, "UZ-DT-003");
    foreign_and_stale(fixture, &receipt_path, receipt).await;
}

fn problem(body: &Value, code: &str) {
    assert_eq!(
        body.pointer("/error_code").expect("response field exists"),
        code
    );
    assert!(
        body.pointer("/request_id")
            .expect("response field exists")
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    assert!(
        body.pointer("/detail")
            .expect("response field exists")
            .as_str()
            .is_some_and(|detail| !detail.is_empty())
    );
}
