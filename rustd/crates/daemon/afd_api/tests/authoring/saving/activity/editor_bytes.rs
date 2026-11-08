//! Complete editor responses stay within the browser limit without hiding recent turns.

use super::{editor::seed_conversations, fixture::Saved};
use serde_json::json;

const USER_TEXT_BYTES: i32 = 8 * 1024;
const ANSWER_TEXT_BYTES: i32 = 112 * 1024;
const CONTEXT_LIMIT_BYTES: usize = 4 * 1024 * 1024;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn editor_bounds_complete_response_bytes_without_losing_recent_turns() {
    let saved = Saved::create(json!({})).await;
    seed_conversations(&saved, &saved.case.router, &saved.case.base.token).await;
    let before = saved.get("/authoring-context").await;
    for chat in before["conversations"].as_array().expect("three chats") {
        seed_large_history(&saved, chat["draft"]["id"].as_str().expect("draft")).await;
    }
    let body = read_bounded_context(&saved).await;
    let chats = body["conversations"].as_array().expect("private chats");
    assert!(chats.iter().any(|chat| chat["truncated"] == true));
    assert_recent_windows(&saved, chats).await;
    saved.case.cleanup().await;
}

async fn seed_large_history(saved: &Saved, id: &str) {
    let mut connection = saved.case.base.db.acquire().await.expect("large history");
    for sequence in 1..=12_i64 {
        sqlx::query("INSERT INTO authoring.messages(id,draft_id,sequence,operation_id,status,user_text,answer_text,created_at,updated_at,finished_at)
            VALUES($1::uuid,$2::uuid,$3,$4::uuid,'completed',repeat('u',$5),repeat('a',$6),now(),now(),now())")
            .bind(crate::authoring::mint().as_str()).bind(id).bind(sequence)
            .bind(crate::authoring::mint().as_str()).bind(USER_TEXT_BYTES).bind(ANSWER_TEXT_BYTES).execute(&mut *connection)
            .await.expect("valid report-sized history");
    }
    sqlx::query("UPDATE authoring.drafts SET last_message_sequence=12 WHERE id=$1::uuid")
        .bind(id)
        .execute(&mut *connection)
        .await
        .expect("history watermark");
}

async fn read_bounded_context(saved: &Saved) -> serde_json::Value {
    use crate::authoring::send;
    use http::{Method, StatusCode};
    let path = format!("{}/authoring-context", saved.item);
    let response = send(
        &saved.case.router,
        Method::GET,
        &path,
        Some(&saved.case.base.token),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("context body");
    assert!(
        bytes.len() <= CONTEXT_LIMIT_BYTES,
        "editor response is {} bytes",
        bytes.len()
    );
    serde_json::from_slice(&bytes).expect("context")
}

async fn assert_recent_windows(saved: &Saved, chats: &[serde_json::Value]) {
    use crate::authoring::{json_body, send};
    use http::{Method, StatusCode};
    for chat in chats {
        let rows = chat["messages"].as_array().expect("recent messages");
        assert_eq!(rows.last().expect("latest turn")["sequence"], "12");
        let Some(cursor) = chat["messages_starting_after"].as_str() else {
            continue;
        };
        let path = format!(
            "{}/{}/messages?limit=25&starting_after={cursor}",
            saved.case.base.collection(),
            chat["draft"]["id"].as_str().expect("draft")
        );
        let response = send(
            &saved.case.router,
            Method::GET,
            &path,
            Some(&saved.case.base.token),
            "",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await["items"], chat["messages"]);
    }
}
