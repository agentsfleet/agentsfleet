//! A durable save request finishes without any browser follow-up writes.

use super::{
    super::{attempt, json_body, mint, send},
    fixture::Case,
};
use afd_auth::{credential::Presented, directory::Digest};
use http::{Method, StatusCode};
use serde_json::{Value, json};

mod atomic;
mod improvement;
mod recovery;
mod refusals;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn ordinary_failed_and_cancelled_messages_never_save() {
    for mode in ["ordinary", "failed", "cancelled"] {
        let mut case = Case::create().await;
        case.fund().await;
        let (item, _) = case.request().await;
        let input = json!({"message":"Keep discussing the setup", "save":mode != "ordinary", "operation_id":mint()});
        accept_message(&case, &item, &input).await;
        let before = read(&case, &item).await;
        let (path, token, mut report) = finish_request(&mut case, &before).await;
        if mode == "failed" {
            report["outcome"] = json!("failed");
            report["failure_reason"] = json!("provider_failed");
        }
        if mode == "cancelled" {
            let response = send(
                &case.router,
                Method::POST,
                &format!("{item}/cancellations"),
                Some(&case.base.token),
                r#"{"through_sequence":"1"}"#,
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
        }
        let response = send(
            &case.router,
            Method::POST,
            &path,
            Some(&token),
            &report.to_string(),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{mode}: {}",
            json_body(response).await
        );
        assert!(
            read(&case, &item).await["linked_fleet_id"].is_null(),
            "{mode}"
        );
        case.cleanup().await;
    }
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn a_checked_save_refusal_keeps_the_answer_and_draft_editable() {
    let mut case = Case::create().await;
    case.fund().await;
    let (_, existing) = case.request().await;
    assert_eq!(
        case.save(mint().as_str(), &existing).await.status(),
        StatusCode::CREATED
    );
    let (item, _) = case.request().await;
    let input = json!({"message":"Save this setup", "save":true, "operation_id":mint()});
    accept_message(&case, &item, &input).await;
    let before = read(&case, &item).await;
    let (path, token, report) = finish_request(&mut case, &before).await;
    report_once(&case, &path, &token, &report).await;
    let after = read(&case, &item).await;
    assert_eq!(after["state"], "editing");
    assert!(after["linked_fleet_id"].is_null());
    let messages = read(&case, &format!("{item}/messages")).await;
    let answer = &messages["items"][1];
    assert_eq!(answer["state"], "completed");
    assert_eq!(answer["text"], "Prepared.");
    assert_eq!(
        answer["save_error_code"],
        afd_core::error_code::AGENTSFLEET_NAME_EXISTS.as_str()
    );
    let edited = super::super::send_with_headers(
        &case.router,
        Method::PATCH,
        &item,
        Some(&case.base.token),
        r#"{"instance_name":"recovered-save"}"#,
        &[(
            http::header::IF_MATCH,
            after["etag"].as_str().expect("current tag"),
        )],
    )
    .await;
    assert_eq!(
        edited.status(),
        StatusCode::OK,
        "save refusal never locks the editor"
    );
    case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn the_terminal_report_saves_once_without_a_browser() {
    let mut case = Case::create().await;
    case.fund().await;
    let (item, _) = case.request().await;
    let operation = mint();
    let input =
        json!({"message":"Save it and run it daily", "save":true, "operation_id":operation});
    accept_message(&case, &item, &input).await;
    let before = read(&case, &item).await;
    let (path, token, report) = finish_request(&mut case, &before).await;
    for _ in 0..2 {
        report_once(&case, &path, &token, &report).await;
    }
    let after = read(&case, &item).await;
    let fleet = after["linked_fleet_id"]
        .as_str()
        .expect("server saved the fleet");
    let saved = read(&case, &format!("{}/{}", case.collection(), fleet)).await;
    assert_eq!(saved["setup_revision"], "1");
    assert_eq!(saved["status"], "active");
    let history = read(
        &case,
        &format!("{}/{fleet}/messages?limit=25", case.collection()),
    )
    .await;
    assert_eq!(
        history["items"].as_array().expect("receipts").len(),
        2,
        "one saved and one ready receipt, even after report replay"
    );
    let changed = json!({"message":input["message"],"save":false,"operation_id":operation});
    let response = send(
        &case.router,
        Method::POST,
        &format!("{item}/messages"),
        Some(&case.base.token),
        &changed.to_string(),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "save intent participates in replay identity"
    );
    case.cleanup().await;
}

async fn read(case: &Case, item: &str) -> Value {
    let response = send(&case.router, Method::GET, item, Some(&case.base.token), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

async fn finish_request(case: &mut Case, draft: &Value) -> (String, String, Value) {
    let runner = mint();
    case.base.own_runner(&runner);
    let token = format!("agt_r{}{}", mint(), mint()).replace('-', "");
    let lease = mint();
    let digest = Digest::of(&Presented::new(&token).expect("fixture runner token"));
    attempt::seed(
        &case.base,
        &runner,
        &lease,
        draft["id"].as_str().expect("draft"),
        digest.as_str(),
    )
    .await;
    let report = json!({"kind":"terminal","fence":"1","report_id":mint(),"outcome":"completed",
        "usage":{"knowledge":"known","calls":0,"input_tokens":0,"output_tokens":0,
            "runtime_ms":0,"provider_cost_nanos":"0"},"answer":"Prepared."});
    (
        format!("/v1/runners/me/authoring-leases/{lease}/reports"),
        token,
        report,
    )
}

async fn accept_message(case: &Case, item: &str, input: &Value) {
    let response = send(
        &case.router,
        Method::POST,
        &format!("{item}/messages"),
        Some(&case.base.token),
        &input.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

async fn report_once(case: &Case, path: &str, token: &str, report: &Value) {
    let response = send(
        &case.router,
        Method::POST,
        path,
        Some(token),
        &report.to_string(),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        json_body(response).await
    );
}
