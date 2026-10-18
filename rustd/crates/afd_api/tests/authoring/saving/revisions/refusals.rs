//! A fresh tag cannot disguise an improvement based on an older setup.

use super::{
    Fleet, Method, StatusCode, Value, header, json, json_body, mint, send, send_with_headers,
};
use afd_core::id::Uuid7;
use futures_util::future::join_all;
use std::time::Duration;

/// Long enough for any healthy interleaving of rival Applies; a deadlock outlasts it.
const BOUND: Duration = Duration::from_secs(30);

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_second_improvement_cannot_overwrite_a_changed_base_or_reuse_another_key() {
    let fleet = Fleet::create().await;
    let (_, first) = fleet.improvement().await;
    let (_, second) = fleet.improvement().await;
    let key = mint();
    let tag = fleet.saved["setup_etag"].as_str().expect("base tag");
    let accepted = fleet.apply(key.as_str(), tag, &first).await;
    assert_eq!(accepted.status(), StatusCode::CREATED);
    let accepted = json_body(accepted).await;
    let stale = fleet.apply(mint().as_str(), tag, &second).await;
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(json_body(stale).await["etag"], accepted["setup_etag"]);
    let changed = fleet
        .apply(
            mint().as_str(),
            accepted["setup_etag"].as_str().expect("new tag"),
            &second,
        )
        .await;
    assert_eq!(changed.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(changed).await["current_state"],
        "source_revision_changed"
    );
    assert_eq!(
        fleet.apply(key.as_str(), tag, &second).await.status(),
        StatusCode::CONFLICT
    );
    assert_eq!(fleet.revision_count().await, 2);
    fleet.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn rename_and_stop_preserve_setup_and_resume_rechecks_the_removed_model() {
    let fleet = Fleet::create().await;
    let response = fleet
        .patch_resource(
            r#"{"name":"resource-renamed","status":"stopped"}"#,
            fleet.saved["etag"].as_str().expect("resource tag"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let stopped = json_body(response).await;
    assert_eq!(stopped["needs_re_review"], false);
    assert_eq!(stopped["setup_etag"], fleet.saved["setup_etag"]);
    sqlx::query("DELETE FROM model.tenant_models WHERE id=$1::uuid")
        .bind(fleet.case.base.model.as_str())
        .execute(
            &mut *fleet
                .case
                .base
                .db
                .acquire()
                .await
                .expect("model connection"),
        )
        .await
        .expect("remove model selection");
    let response = fleet
        .patch_resource(
            r#"{"status":"active"}"#,
            stopped["etag"].as_str().expect("stopped tag"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let current = json_body(response).await;
    assert_eq!(
        current["status"], "installing",
        "removed model cannot resume runtime work"
    );
    assert_eq!(current["needs_re_review"], true);
    fleet.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_stale_review_digest_refuses_apply_without_side_effects_or_spending_its_key() {
    let fleet = Fleet::create().await;
    let (draft, reviewed) = fleet.improvement().await;
    let current = edit_after_review(&fleet, &draft).await;
    let fresh = json!({"draft_candidate_id":current["candidate"]["id"],
        "review_digest":current["review"]["review_digest"]});
    assert_ne!(fresh["review_digest"], reviewed["review_digest"]);
    let mut stale = fresh.clone();
    stale["review_digest"] = reviewed["review_digest"].clone();
    let key = mint();
    let tag = fleet.saved["setup_etag"].as_str().expect("base tag");
    let refused = fleet.apply(key.as_str(), tag, &stale).await;
    let status = refused.status();
    let body = json_body(refused).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error_code"], "UZ-AUTHOR-006");
    assert_eq!(body["review_digest"], fresh["review_digest"], "{body}");
    let unchanged = fleet.current().await;
    assert_eq!(unchanged["setup_etag"], fleet.saved["setup_etag"]);
    assert_eq!(unchanged["definition"], fleet.saved["definition"]);
    assert_eq!(fleet.revision_count().await, 1);
    assert_eq!(draft_state(&fleet, &draft).await, "editing");
    let applied = fleet.apply(key.as_str(), tag, &fresh).await;
    assert_eq!(
        applied.status(),
        StatusCode::CREATED,
        "the refusal spent no key"
    );
    assert_eq!(json_body(applied).await["setup_revision"], "2");
    fleet.case.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn rival_applies_on_one_base_leave_exactly_one_winner_and_refuse_the_rest() {
    const RIVALS: usize = 4;
    let fleet = Fleet::create().await;
    let mut rivals = Vec::new();
    for _ in 0..RIVALS {
        let (draft, body) = fleet.improvement().await;
        rivals.push((draft, body, mint()));
    }
    let answers = apply_together(&fleet, &rivals).await;
    let winners: Vec<_> = answers
        .iter()
        .filter(|(status, _)| *status == StatusCode::CREATED)
        .collect();
    assert_eq!(
        winners.len(),
        1,
        "exactly one Apply wins the base: {answers:?}"
    );
    let winner = &winners[0].1;
    assert_eq!(winner["setup_revision"], "2");
    for (status, body) in answers
        .iter()
        .filter(|(status, _)| *status != StatusCode::CREATED)
    {
        assert_eq!(*status, StatusCode::PRECONDITION_FAILED, "{body}");
        assert_eq!(
            body["etag"], winner["setup_etag"],
            "a loser learns the new base"
        );
    }
    assert_eq!(fleet.revision_count().await, 2);
    let mut editing = 0;
    for (draft, _, _) in &rivals {
        if draft_state(&fleet, draft).await == "editing" {
            editing += 1;
        }
    }
    assert_eq!(editing, RIVALS - 1, "only the winner's draft closed");
    fleet.case.cleanup().await;
}

/// Sends every rival's Apply against the original base tag at once, bounded,
/// and answers each status with its body.
async fn apply_together(
    fleet: &Fleet,
    rivals: &[(String, Value, Uuid7)],
) -> Vec<(StatusCode, Value)> {
    let tag = fleet.saved["setup_etag"].as_str().expect("base tag");
    let applying = join_all(
        rivals
            .iter()
            .map(|(_, body, key)| fleet.apply(key.as_str(), tag, body)),
    );
    let mut answers = Vec::new();
    for response in tokio::time::timeout(BOUND, applying)
        .await
        .expect("rival Applies finish without a deadlock")
    {
        let status = response.status();
        answers.push((status, json_body(response).await));
    }
    answers
}

/// Edits `draft` again after its review, under its current tag, and answers
/// the draft with its new candidate and review.
async fn edit_after_review(fleet: &Fleet, draft: &str) -> Value {
    let router = &fleet.case.router;
    let token = Some(fleet.case.base.token.as_str());
    let read = send(router, Method::GET, draft, token, "").await;
    let tag = read.headers()[header::ETAG]
        .to_str()
        .expect("draft tag")
        .to_owned();
    let edit = r#"{"instruction_edits":[{"start_byte":0,"end_byte":0,"text":"Cite the failing commit.\n"}]}"#;
    let response = send_with_headers(
        router,
        Method::PATCH,
        draft,
        token,
        edit,
        &[(header::IF_MATCH, &tag)],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

/// The lifecycle state `draft` currently reports to its creator.
async fn draft_state(fleet: &Fleet, draft: &str) -> String {
    let token = Some(fleet.case.base.token.as_str());
    let response = send(&fleet.case.router, Method::GET, draft, token, "").await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await["state"]
        .as_str()
        .expect("draft state")
        .to_owned()
}
