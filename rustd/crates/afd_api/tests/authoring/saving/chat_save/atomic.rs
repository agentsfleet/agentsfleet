//! Immediate and deferred receipt refusals roll back the answer and setup together.

use super::super::activity::fault::Refusal;
use super::*;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn a_failed_save_commit_can_replay_the_same_report_without_partial_rows() {
    refuse_and_retry(true).await;
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn a_failed_save_write_rolls_back_the_answer_for_report_retry() {
    refuse_and_retry(false).await;
}

async fn refuse_and_retry(at_commit: bool) {
    let mut case = Case::create().await;
    case.fund().await;
    let (item, _) = case.request().await;
    let input = json!({"message":"Save this setup", "save":true,"operation_id":mint()});
    let response = send(
        &case.router,
        Method::POST,
        &format!("{item}/messages"),
        Some(&case.base.token),
        &input.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let before = read(&case, &item).await;
    let (path, token, report) = finish_request(&mut case, &before).await;
    refuse_save(&case, &path, &token, &report, &item, at_commit).await;
    for _ in 0..2 {
        let response = send(
            &case.router,
            Method::POST,
            &path,
            Some(&token),
            &report.to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
    assert_eq!(counts(&case).await, (1, 2, 1));
    case.cleanup().await;
}

async fn counts(case: &Case) -> (i64, i64, i64) {
    sqlx::query_as(
        "SELECT
        (SELECT count(*) FROM fleet.fleets WHERE workspace_id=$1::uuid),
        (SELECT count(*) FROM fleet.events WHERE workspace_id=$1::uuid),
        (SELECT count(*) FROM billing.authoring_report_receipts r
          JOIN authoring.attempts a ON a.id=r.attempt_id WHERE a.workspace_id=$1::uuid)",
    )
    .bind(case.base.workspace.as_str())
    .fetch_one(
        &mut *case
            .base
            .db
            .acquire()
            .await
            .expect("atomic counts connection"),
    )
    .await
    .expect("workspace-scoped save counts")
}

async fn refuse_save(
    case: &Case,
    path: &str,
    token: &str,
    report: &Value,
    item: &str,
    at_commit: bool,
) {
    let fault = if at_commit {
        Refusal::at_commit(case.base.owner_url(), &case.base.workspace, "setup_saved").await
    } else {
        Refusal::install(case.base.owner_url(), &case.base.workspace, "setup_saved").await
    };
    let response = send(
        &case.router,
        Method::POST,
        path,
        Some(token),
        &report.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    fault.assert_fired_once().await;
    assert_eq!(counts(case).await, (0, 0, 0));
    let messages = read(case, &format!("{item}/messages")).await;
    assert_eq!(
        messages["items"].as_array().expect("unfinished turn").len(),
        1
    );
    assert_eq!(messages["items"][0]["state"], "running");
    drop(fault);
}
