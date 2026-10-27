//! HTTP Save reports readiness honestly and retries one retained resource.

use super::{
    super::{json_body, mint, send, send_with_headers},
    fixture::{Case, runtime_history},
};
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn save_retries_the_same_fleet_after_funding_without_copying_private_chat() {
    let case = Case::create().await;
    let (draft, request) = case.request().await;
    let key = mint();
    let (item, first) = unfunded_save_waits_on_the_wallet(&case, key.as_str(), &request).await;
    case.fund().await;
    let response = case.save(key.as_str(), &request).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()[header::LOCATION], item);
    let tag = response.headers()[header::ETAG]
        .to_str()
        .expect("resource tag")
        .to_owned();
    let saved = json_body(response).await;
    assert_eq!(saved["id"], first["id"]);
    assert_eq!(saved["status"], "active");
    assert_eq!(saved["definition"]["instructions_md"], "Investigate.\n");
    assert_only_setup_receipts(&case, &item, &saved).await;
    assert_eq!(saved["budget_used_nanos"], "0");
    assert_eq!(saved["schedules"], json!([]));
    for key in ["messages", "authoring_model_ref", "draft_id", "transcript"] {
        assert!(saved.get(key).is_none(), "shared fleet leaks {key}");
    }
    let current = send(&case.router, Method::GET, &item, Some(&case.base.token), "").await;
    assert_eq!(current.headers()[header::ETAG], tag);
    assert_eq!(json_body(current).await["definition"], saved["definition"]);
    assert_private_draft_closed(&case, &draft).await;
    assert_rename_preserves_setup(&case, &item, &tag, &saved).await;
    case.cleanup().await;
}

async fn assert_only_setup_receipts(case: &Case, item: &str, saved: &Value) {
    let messages = send(
        &case.router,
        Method::GET,
        &format!("{item}/messages"),
        Some(&case.base.token),
        "",
    )
    .await;
    assert_eq!(messages.status(), StatusCode::OK);
    let page = json_body(messages).await;
    assert_eq!(
        runtime_history(
            &page,
            saved["id"].as_str().expect("fleet id"),
            case.base.workspace.as_str()
        ),
        Vec::<Value>::new()
    );
    // pin test: literal is the contract; this is a lifetime history count.
    assert_eq!(
        saved["events_processed"], "2",
        "the lifetime count includes both verified receipts"
    );
}

/// Saves before the wallet is funded: the fleet is created but stays
/// `installing` with a pending wallet check. Answers its location and body.
async fn unfunded_save_waits_on_the_wallet(
    case: &Case,
    key: &str,
    request: &Value,
) -> (String, Value) {
    let response = case.save(key, request).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let item = response.headers()[header::LOCATION]
        .to_str()
        .expect("fleet location")
        .to_owned();
    let first = json_body(response).await;
    assert_eq!(first["status"], "installing");
    assert_eq!(first["start"], json!({"kind":"none"}));
    assert!(
        first["setup"]["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .any(|c| c["kind"] == "wallet" && c["state"] == "pending")
    );
    (item, first)
}

/// The private draft the Save came from reads back `closed`.
async fn assert_private_draft_closed(case: &Case, draft: &str) {
    let private = send(&case.router, Method::GET, draft, Some(&case.base.token), "").await;
    let private_status = private.status();
    let private = json_body(private).await;
    assert_eq!(private_status, StatusCode::OK, "{private}");
    assert_eq!(private["state"], "closed");
}

/// Renaming the saved fleet changes its name and leaves its setup tag alone.
async fn assert_rename_preserves_setup(case: &Case, item: &str, tag: &str, saved: &Value) {
    let renamed = send_with_headers(
        &case.router,
        Method::PATCH,
        item,
        Some(&case.base.token),
        r#"{"name":"http-renamed"}"#,
        &[(header::IF_MATCH, tag)],
    )
    .await;
    assert_eq!(renamed.status(), StatusCode::OK);
    let renamed = json_body(renamed).await;
    assert_eq!(renamed["name"], "http-renamed");
    assert_eq!(
        renamed["setup_etag"], saved["setup_etag"],
        "rename preserves setup identity"
    );
}
