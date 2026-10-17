//! A blocked first run resumes under its fleet identity after the client reloads.

use super::{
    super::{json_body, mint, refusal::Refusal, send, send_with_headers},
    fixture::Case,
};
use afd_auth::Scope;
use axum::{Router, response::Response};
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

const ADMISSIONS: &str = "SELECT count(*) FROM core.fleet_admissions WHERE fleet_id=$1::uuid";
/// The first run every Save-and-run case asks for.
const RUN_NOW: &str = "Investigate the failed build.";

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn resume_after_reload_recovers_a_capacity_blocked_initial_run_once() {
    let case = Case::with_budgets(afd_admission::Budgets {
        replay_backlog: 0,
        ..afd_admission::Budgets::default()
    })
    .await;
    case.fund().await;
    let blocked = save_with_a_blocked_first_run(&case).await;
    let healthy = case
        .base
        .services(&Scope::ALL)
        .with_fleet_queue(case.base.db.clone(), case.queue.clone())
        .router();
    let path = format!(
        "{}/{}",
        case.collection(),
        blocked["id"].as_str().expect("fleet id")
    );
    let current =
        json_body(send(&healthy, Method::GET, &path, Some(&case.base.token), "").await).await;
    let response = resume(
        &case,
        &healthy,
        &path,
        current["etag"].as_str().expect("resource tag"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let resumed = json_body(response).await;
    assert_eq!(resumed["id"], blocked["id"]);
    assert_eq!(resumed["setup_revision"], blocked["setup_revision"]);
    assert_eq!(resumed["start"]["state"], "accepted");
    assert!(resumed["start"]["event_id"].is_string());
    assert_eq!(count(&case, &resumed).await, 1);
    let repeated = resume(
        &case,
        &healthy,
        &path,
        resumed["etag"].as_str().expect("current tag"),
    )
    .await;
    assert_eq!(repeated.status(), StatusCode::OK);
    assert_eq!(json_body(repeated).await["start"], resumed["start"]);
    assert_eq!(count(&case, &resumed).await, 1);
    case.cleanup().await;
}

/// Saves with `run_now` while the replay backlog has no room: the fleet is
/// active, its first run is `blocked`, and nothing was admitted.
async fn save_with_a_blocked_first_run(case: &Case) -> Value {
    let (_, mut request) = case.request().await;
    request["start"] = json!({"kind":"run_now","message":RUN_NOW});
    let key = mint();
    let response = case.save(key.as_str(), &request).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let blocked = json_body(response).await;
    assert_eq!(blocked["status"], "active");
    assert_eq!(blocked["start"]["state"], "blocked");
    assert_eq!(count(case, &blocked).await, 0);
    blocked
}

/// Resumes the fleet at `path` through `router` under `tag`.
async fn resume(case: &Case, router: &Router, path: &str, tag: &str) -> Response {
    send_with_headers(
        router,
        Method::PATCH,
        path,
        Some(&case.base.token),
        r#"{"status":"active"}"#,
        &[(header::IF_MATCH, tag)],
    )
    .await
}

async fn count(case: &Case, fleet: &Value) -> i64 {
    sqlx::query_scalar(ADMISSIONS)
        .bind(fleet["id"].as_str().expect("fleet id"))
        .fetch_one(&mut *case.base.db.acquire().await.expect("admission connection"))
        .await
        .expect("count this fleet's durable admissions")
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_refused_receipt_keeps_the_first_run_pending_through_reload_until_one_retry_accepts_it() {
    let case = Case::create().await;
    let (_, mut request) = case.request().await;
    request["start"] = json!({"kind":"run_now","message":RUN_NOW});
    let key = mint();
    let (path, fleet) = retained(&case, key.as_str(), &request).await;
    let refusal = Refusal::install(
        &case.base.owner_url(),
        "core.fleet_admissions",
        "receipt",
        &fleet,
        &format!("NEW.fleet_id='{fleet}'::uuid AND NEW.receipt IS NOT NULL"),
        "RAISE EXCEPTION 'fixture refusal'",
    )
    .await;
    case.fund().await;
    let interrupted = case.save(key.as_str(), &request).await;
    let status = interrupted.status();
    let body = json_body(interrupted).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["status"], "active");
    assert_eq!(
        body["start"]["state"], "pending",
        "an unsaved receipt is not accepted"
    );
    let event = pending_after_reload(&case, &path).await;
    assert_eq!(
        body["start"]["event_id"],
        event.as_str(),
        "the identity is stable"
    );
    drop(refusal);
    for _retry in 0..2 {
        let response = case.save(key.as_str(), &request).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let resumed = json_body(response).await;
        assert_eq!(resumed["id"], fleet.as_str());
        assert_eq!(resumed["start"]["state"], "accepted");
        assert_eq!(resumed["start"]["event_id"], event.as_str());
        assert_eq!(thread(&case, &path).await, std::slice::from_ref(&event));
        assert_eq!(
            count(&case, &resumed).await,
            1,
            "a retry never admits a second run"
        );
    }
    case.cleanup().await;
}

/// Saves `request` unfunded, so the fleet is retained `installing` and its
/// identity is known before a fault is armed. Answers its location and ID.
async fn retained(case: &Case, key: &str, request: &Value) -> (String, String) {
    let response = case.save(key, request).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let path = response.headers()[header::LOCATION]
        .to_str()
        .expect("fleet location")
        .to_owned();
    let saved = json_body(response).await;
    assert_eq!(saved["status"], "installing");
    let fleet = saved["id"].as_str().expect("fleet id").to_owned();
    (path, fleet)
}

/// What a reload shows after the interrupted Save: an active fleet whose
/// first run is pending under its stable identity, listed once as a message.
async fn pending_after_reload(case: &Case, path: &str) -> String {
    let response = send(&case.router, Method::GET, path, Some(&case.base.token), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let reloaded = json_body(response).await;
    assert_eq!(reloaded["status"], "active");
    assert_eq!(reloaded["start"]["state"], "pending", "{reloaded}");
    let event = reloaded["start"]["event_id"]
        .as_str()
        .expect("a pending run keeps its event identity")
        .to_owned();
    assert_eq!(thread(case, path).await, std::slice::from_ref(&event));
    assert_eq!(count(case, &reloaded).await, 1);
    event
}

/// The event IDs the fleet's message thread lists, newest first.
async fn thread(case: &Case, path: &str) -> Vec<String> {
    let messages = format!("{path}/messages");
    let response = send(
        &case.router,
        Method::GET,
        &messages,
        Some(&case.base.token),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let listed = json_body(response).await;
    listed["items"]
        .as_array()
        .expect("message page")
        .iter()
        .map(|item| item["event_id"].as_str().expect("event id").to_owned())
        .collect()
}
