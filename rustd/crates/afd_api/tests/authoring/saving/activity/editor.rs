//! One creator-private editor read includes shared readiness and filtered receipts.

use super::fixture::Saved;
use serde_json::json;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn editor_context_requires_the_private_draft_write_scope() {
    use crate::authoring::send;
    use afd_auth::Scope;
    use http::{Method, StatusCode};
    let saved = Saved::create(json!({})).await;
    let path = format!("{}/authoring-context", saved.item);
    for (scope, expected) in [
        (Scope::FleetRead, StatusCode::FORBIDDEN),
        (Scope::FleetWrite, StatusCode::OK),
    ] {
        let router = saved.case.base.services(&[scope]).router();
        let response = send(
            &router,
            Method::GET,
            &path,
            Some(&saved.case.base.token),
            "",
        )
        .await;
        assert_eq!(response.status(), expected, "private draft authorization");
    }
    saved.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn editor_context_combines_fleet_private_draft_and_receipts() {
    let saved = Saved::create(json!({})).await;
    let view = saved.get("/authoring-context").await;
    assert_eq!(view["fleet"]["id"], saved.fleet.as_str());
    assert_eq!(view["fleet"]["setup_revision"], "1");
    let chats = view["conversations"]
        .as_array()
        .expect("creator conversations");
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0]["draft"]["linked_fleet_id"], saved.fleet.as_str());
    assert_eq!(
        view["activities"]["items"]
            .as_array()
            .expect("receipts")
            .len(),
        2
    );
    assert!(view["next_cursor"].is_null());
    saved.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn editor_history_pages_bind_creator_and_never_expose_another_members_chat() {
    use crate::authoring::{privacy, send};
    use http::{Method, StatusCode};
    let mut saved = Saved::create(json!({})).await;
    let member = privacy::member(&mut saved.case.base).await;
    let router = privacy::router(&saved.case.base, &member);
    seed_conversations(&saved, &router, &member.token).await;
    let path = format!("{}/authoring-context", saved.item);
    let first = read_context(&router, &path, &saved.case.base.token).await;
    assert_eq!(
        first["conversations"].as_array().expect("first page").len(),
        3
    );
    let cursor = first["next_cursor"]
        .as_str()
        .expect("two older owner conversations");
    let continued = format!("{path}?starting_after={cursor}");
    let second = read_context(&router, &continued, &saved.case.base.token).await;
    assert_eq!(
        second["conversations"]
            .as_array()
            .expect("second page")
            .len(),
        2
    );
    assert!(second["next_cursor"].is_null());
    let refused = send(&router, Method::GET, &continued, Some(&member.token), "").await;
    assert_eq!(
        refused.status(),
        StatusCode::BAD_REQUEST,
        "a cursor cannot cross creators"
    );
    let own = read_context(&router, &path, &member.token).await;
    assert_private_conversations(&first, &second, &own);
    saved.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn editor_returns_a_recent_window_with_a_reusable_message_boundary() {
    use crate::authoring::{json_body, send};
    use http::{Method, StatusCode};
    let saved = Saved::create(json!({})).await;
    let context = saved.get("/authoring-context").await;
    let id = context["conversations"][0]["draft"]["id"]
        .as_str()
        .expect("own draft");
    seed_recent_history(&saved, id).await;
    let context = saved.get("/authoring-context").await;
    let chat = &context["conversations"][0];
    assert_eq!(chat["truncated"], true);
    let rows = chat["messages"].as_array().expect("recent messages");
    assert_eq!(
        rows.len(),
        24,
        "both roles from the twelve most recent turns"
    );
    assert_eq!(rows[0]["sequence"], "4");
    assert_eq!(rows[23]["sequence"], "15");
    let cursor = chat["messages_starting_after"]
        .as_str()
        .expect("recent boundary");
    let path = format!(
        "{}/{id}/messages?limit=25&starting_after={cursor}",
        saved.case.base.collection()
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
    saved.case.cleanup().await;
}

pub(super) async fn seed_conversations(saved: &Saved, router: &axum::Router, member_token: &str) {
    use crate::authoring::{json_body, mint, send};
    use http::{Method, StatusCode};
    for token in [saved.case.base.token.as_str(), member_token] {
        for _ in 0..4 {
            let mut request = saved.case.base.request(&mint());
            request.as_object_mut().expect("request").remove("message");
            request["source"] =
                json!({"kind":"fleet", "fleet_id":saved.fleet, "setup_revision":"1"});
            let response = send(
                router,
                Method::POST,
                &saved.case.base.collection(),
                Some(token),
                &request.to_string(),
            )
            .await;
            assert_eq!(
                response.status(),
                StatusCode::CREATED,
                "{}",
                json_body(response).await
            );
        }
    }
}

async fn read_context(router: &axum::Router, path: &str, token: &str) -> serde_json::Value {
    use crate::authoring::{json_body, send};
    use http::{Method, StatusCode, header};
    let response = send(router, Method::GET, path, Some(token), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "private, no-store"
    );
    json_body(response).await
}

fn assert_private_conversations(
    first: &serde_json::Value,
    second: &serde_json::Value,
    own: &serde_json::Value,
) {
    let owner_ids = first["conversations"]
        .as_array()
        .expect("first")
        .iter()
        .chain(second["conversations"].as_array().expect("second"))
        .map(|item| item["draft"]["id"].clone())
        .collect::<Vec<_>>();
    for item in own["conversations"]
        .as_array()
        .expect("member conversations")
    {
        assert!(!owner_ids.contains(&item["draft"]["id"]));
    }
}

async fn seed_recent_history(saved: &Saved, id: &str) {
    use crate::authoring::mint;
    let mut connection = saved.case.base.db.acquire().await.expect("seed history");
    for sequence in 1..=15_i64 {
        sqlx::query("INSERT INTO authoring.messages(id,draft_id,sequence,operation_id,status,user_text,answer_text,created_at,updated_at,finished_at)
            VALUES($1::uuid,$2::uuid,$3,$4::uuid,'completed','Earlier private request','Durable answer',now(),now(),now())")
            .bind(mint().as_str()).bind(id).bind(sequence).bind(mint().as_str())
            .execute(&mut *connection).await.expect("settled history");
    }
    sqlx::query("UPDATE authoring.drafts SET last_message_sequence=15 WHERE id=$1::uuid")
        .bind(id)
        .execute(&mut *connection)
        .await
        .expect("history watermark");
    drop(connection);
}
