//! Refused save intent preserves the answer, and missing names use the definition.

use super::super::super::send_with_headers;
use super::*;
use http::header;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn initial_save_requires_a_message_without_spending_its_operation() {
    let case = Case::create().await;
    case.fund().await;
    let mut input = case.base.request(&mint());
    input.as_object_mut().expect("input").remove("message");
    input["save"] = json!(true);
    let refused = create(&case, &input).await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM authoring.drafts WHERE workspace_id=$1::uuid")
            .bind(case.base.workspace.as_str())
            .fetch_one(&mut *case.base.db.acquire().await.expect("draft counts"))
            .await
            .expect("draft count");
    assert_eq!(count, 0);
    input["message"] = json!("Save this setup");
    assert_eq!(create(&case, &input).await.status(), StatusCode::CREATED);
    case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn an_unnamed_instance_saves_with_its_definition_name() {
    let mut case = Case::create().await;
    case.fund().await;
    let mut input = case.base.request(&mint());
    input.as_object_mut().expect("input").remove("message");
    let created = create(&case, &input).await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let item = created.headers()[header::LOCATION]
        .to_str()
        .expect("location")
        .to_owned();
    let created = json_body(created).await;
    let edited = send_with_headers(
        &case.router,
        Method::PATCH,
        &item,
        Some(&case.base.token),
        &json!({"runtime_model_ref":case.base.model}).to_string(),
        &[(header::IF_MATCH, created["etag"].as_str().expect("tag"))],
    )
    .await;
    assert_eq!(edited.status(), StatusCode::OK);
    accept_message(
        &case,
        &item,
        &json!({"message":"Save this setup","save":true,"operation_id":mint()}),
    )
    .await;
    let draft = read(&case, &item).await;
    let (path, token, report) = finish_request(&mut case, &draft).await;
    report_once(&case, &path, &token, &report).await;
    let saved = read(&case, &item).await;
    let fleet = read(
        &case,
        &format!(
            "{}/{}",
            case.collection(),
            saved["linked_fleet_id"].as_str().expect("fleet")
        ),
    )
    .await;
    assert_eq!(fleet["name"], "checkout-watch");
    assert_eq!(fleet["status"], "active");
    case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn retired_policy_refuses_the_save_without_discarding_the_answer() {
    let mut case = Case::create().await;
    case.fund().await;
    let (item, _) = case.request().await;
    let draft = read(&case, &item).await;
    let retired = mint();
    let mut connection = case.base.db.acquire().await.expect("candidate connection");
    sqlx::query("INSERT INTO authoring.candidates(id,draft_id,revision,definition,canonical_digest,policy_semantics,created_at)
        SELECT $2::uuid,c.draft_id,c.revision+1,c.definition,c.canonical_digest,'retired',now()
        FROM authoring.candidates c JOIN authoring.drafts d ON d.head_candidate_id=c.id WHERE d.id=$1::uuid")
        .bind(draft["id"].as_str().expect("draft")).bind(retired.as_str())
        .execute(&mut *connection).await.expect("older daemon candidate");
    sqlx::query("UPDATE authoring.drafts SET head_candidate_id=$2::uuid WHERE id=$1::uuid")
        .bind(draft["id"].as_str().expect("draft"))
        .bind(retired.as_str())
        .execute(&mut *connection)
        .await
        .expect("retired candidate head");
    drop(connection);
    accept_message(
        &case,
        &item,
        &json!({"message":"Save this setup","save":true,"operation_id":mint()}),
    )
    .await;
    let before = read(&case, &item).await;
    let (path, token, report) = finish_request(&mut case, &before).await;
    report_once(&case, &path, &token, &report).await;
    assert_refused(
        &case,
        &item,
        afd_core::error_code::AUTHORING_POLICY.as_str(),
    )
    .await;
    case.cleanup().await;
}

async fn create(case: &Case, input: &Value) -> axum::response::Response {
    send(
        &case.router,
        Method::POST,
        &case.base.collection(),
        Some(&case.base.token),
        &input.to_string(),
    )
    .await
}

pub(super) async fn assert_refused(case: &Case, item: &str, code: &str) {
    let after = read(case, item).await;
    assert_eq!(after["state"], "editing");
    let messages = read(case, &format!("{item}/messages")).await;
    let answer = &messages["items"][1];
    assert_eq!(answer["state"], "completed");
    assert_eq!(answer["text"], "Prepared.");
    assert_eq!(answer["save_error_code"], code);
}
