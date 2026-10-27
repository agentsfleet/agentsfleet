//! A durable chat save applies a revision to its existing fleet.

use super::{Case, finish_request, json_body, mint, read, report_once, send};
use http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn saved_fleet_turn_applies_once_and_replays_without_another_revision() {
    let mut case = Case::create().await;
    case.fund().await;
    let (_, request) = case.request().await;
    let saved = case.save(mint().as_str(), &request).await;
    assert_eq!(saved.status(), StatusCode::CREATED);
    let saved = json_body(saved).await;
    let mut input = case.base.request(&mint());
    input["source"] = json!({"kind":"fleet","fleet_id":saved["id"],"setup_revision":"1"});
    input["message"] = json!("Save these changes");
    input["save"] = json!(true);
    let created = send(
        &case.router,
        Method::POST,
        &case.base.collection(),
        Some(&case.base.token),
        &input.to_string(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = json_body(created).await;
    let (path, token, report) = finish_request(&mut case, &created).await;
    for _ in 0..2 {
        report_once(&case, &path, &token, &report).await;
    }
    let fleet_id = saved["id"].as_str().expect("fleet");
    let fleet = read(&case, &format!("{}/{fleet_id}", case.collection())).await;
    assert_eq!(fleet["id"], saved["id"]);
    assert_eq!(fleet["setup_revision"], "2");
    let draft_id = created["id"].as_str().expect("draft");
    let draft = read(&case, &format!("{}/{draft_id}", case.base.collection())).await;
    assert_eq!(draft["state"], "closed");
    let history = read(
        &case,
        &format!(
            "{}/{}/messages?event_type=activity",
            case.collection(),
            saved["id"].as_str().expect("fleet")
        ),
    )
    .await;
    assert_eq!(history["items"].as_array().expect("receipt pairs").len(), 4);
    case.cleanup().await;
}
