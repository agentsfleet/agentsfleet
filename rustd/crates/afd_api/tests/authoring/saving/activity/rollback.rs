//! Refusing the receipt after the mutation proves both durable writes share a commit.

use super::super::{
    super::{json_body, mint, send},
    fixture::Case,
};
use super::{fault::Refusal, fixture::*, streams::Reader};
use afd_vault::SecretName;
use http::{Method, StatusCode};
use serde_json::{Value, json};

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn refused_setup_saved_receipt_rolls_back_allocation_and_retries_once() {
    let case = Case::create().await;
    case.fund().await;
    let (draft, request) = case.request().await;
    let key = mint();
    let fault = Refusal::install(case.base.owner_url(), &case.base.workspace, "setup_saved").await;
    let refused = case.save(key.as_str(), &request).await;
    assert_receipt_refused(refused).await;
    fault.assert_fired_once().await;
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.fleets WHERE workspace_id=$1::uuid")
            .bind(case.base.workspace.as_str())
            .fetch_one(&mut *case.base.db.acquire().await.expect("allocation connection"))
            .await
            .expect("allocation count");
    assert_eq!(rows, 0);
    let reopened = send(
        &case.router,
        Method::GET,
        &draft,
        Some(&case.base.token),
        "",
    )
    .await;
    assert_eq!(json_body(reopened).await["state"], "editing");
    drop(fault);
    let first = case.save(key.as_str(), &request).await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first = json_body(first).await;
    let replay = case.save(key.as_str(), &request).await;
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(json_body(replay).await["id"], first["id"]);
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fleet.events WHERE workspace_id=$1::uuid AND event_type='activity'",
    )
    .bind(case.base.workspace.as_str())
    .fetch_one(&mut *case.base.db.acquire().await.expect("receipt connection"))
    .await
    .expect("receipt count");
    assert_eq!(receipts, 2, "one saved/ready pair after retry and replay");
    case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn refused_ready_receipt_keeps_installing_then_retry_activates_once() {
    let saved = Saved::create(json!({"credentials":[SERVICE]})).await;
    assert_eq!(
        saved.access(Method::POST, "fixture-access").await.status(),
        StatusCode::CREATED
    );
    let fault = Refusal::install(
        saved.case.base.owner_url(),
        &saved.case.base.workspace,
        "setup_ready",
    )
    .await;
    assert_receipt_refused(saved.activate().await).await;
    fault.assert_fired_once().await;
    assert_eq!(saved.get("").await["status"], "installing");
    assert_receipts(
        &saved.receipts().await,
        &[saved_receipt(), access_receipt()],
    );
    drop(fault);
    assert_eq!(saved.activate().await.status(), StatusCode::OK);
    assert_eq!(saved.activate().await.status(), StatusCode::OK);
    assert_eq!(saved.get("").await["status"], "active");
    assert_receipts(
        &saved.receipts().await,
        &[saved_receipt(), access_receipt(), ready_receipt()],
    );
    saved.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn refused_access_receipt_rolls_back_secret_create_and_replace_before_retry() {
    let saved = Saved::create(json!({"credentials":[SERVICE]})).await;
    let mut reader = Reader::open(&saved, &saved.fleet).await;
    for (method, previous, next) in [
        (Method::POST, None, "fixture-first"),
        (Method::PUT, Some("fixture-first"), "fixture-replaced"),
    ] {
        let fault = Refusal::install(
            saved.case.base.owner_url(),
            &saved.case.base.workspace,
            "access_saved",
        )
        .await;
        let before = saved.receipts().await;
        assert_receipt_refused(saved.access(method.clone(), next).await).await;
        fault.assert_fired_once().await;
        assert_secret(&saved, previous).await;
        assert_receipts(&saved.receipts().await, &before);
        reader.quiet().await;
        drop(fault);
        let expected = if method == Method::POST {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        };
        assert_eq!(saved.access(method, next).await.status(), expected);
        reader.changed().await;
        reader.quiet().await;
        assert_secret(&saved, Some(next)).await;
        let mut expected = before;
        expected.push(access_receipt());
        assert_receipts(&saved.receipts().await, &expected);
    }
    saved.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn refused_memory_receipt_rolls_back_both_grants_and_retry_records_one_change() {
    let saved = Saved::create(json!({})).await;
    let mut reader = Reader::open(&saved, &saved.fleet).await;
    let before = saved.grants().await;
    let fault = Refusal::install(
        saved.case.base.owner_url(),
        &saved.case.base.workspace,
        "memory_sharing_changed",
    )
    .await;
    assert_receipt_refused(saved.memory(true, true).await).await;
    fault.assert_fired_once().await;
    assert_eq!(saved.grants().await, before);
    assert_receipts(&saved.receipts().await, &[saved_receipt(), ready_receipt()]);
    reader.quiet().await;
    drop(fault);
    assert_eq!(saved.memory(true, true).await.status(), StatusCode::OK);
    reader.changed().await;
    assert_eq!(saved.memory(true, true).await.status(), StatusCode::OK);
    reader.quiet().await;
    assert_eq!(saved.grants().await, json!({"read":true,"publish":true}));
    assert_receipts(
        &saved.receipts().await,
        &[saved_receipt(), ready_receipt(), memory_receipt(true, true)],
    );
    saved.case.cleanup().await;
}

async fn assert_receipt_refused(response: axum::response::Response) {
    let status = response.status();
    let body = json_body(response).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(
        body["error_code"],
        afd_core::error_code::INTERNAL_DB_QUERY.as_str()
    );
    assert!(
        !body.to_string().contains("injected receipt refusal"),
        "private database fault stays private"
    );
}

pub(super) async fn assert_secret(saved: &Saved, expected: Option<&str>) {
    let vault = crate::harness::vault(saved.case.base.db.clone());
    let stored = vault
        .load(
            &saved.case.base.workspace,
            &SecretName::parse(SERVICE).expect("fixture name"),
        )
        .await
        .expect("reload sealed access");
    let actual = stored
        .as_ref()
        .map(|bytes| serde_json::from_slice::<Value>(bytes.expose()).expect("fixture secret JSON"));
    assert_eq!(actual, expected.map(|key| json!({"api_key":key})));
}
