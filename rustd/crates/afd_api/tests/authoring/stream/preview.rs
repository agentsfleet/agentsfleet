//! A valid ordered marker proves earlier rejected pushes never reach the private body.

use super::live::{Live, closed, data, frame};
use serde_json::{Value, json};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn running_private_answer_arrives_before_durable_completion() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    let expected = live.delta("incremental answer 日本語");
    live.publish(&expected);
    let received = frame(&mut body).await;
    assert!(received.contains("event: message.delta\n"), "{received}");
    assert_eq!(data(&received), expected);
    let escaped = "\0".repeat(4096);
    let expected = live.delta(&escaped);
    live.publish(&expected);
    assert_eq!(data(&frame(&mut body).await), expected);
    let response = crate::harness::send(
        &live.router,
        http::Method::GET,
        &format!("{}/messages", live.item),
        Some(&live.f.token),
        "",
    )
    .await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let messages = crate::harness::json_body(response).await;
    assert!(!messages.to_string().contains("incremental answer"));
    drop(body);
    assert_eq!(live.hub.readers(&live.channel()), 0);
    live.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn running_answer_remains_visible_behind_twenty_five_queued_messages() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    for _ in 0..25 {
        let response = crate::harness::send(
            &live.router,
            http::Method::POST,
            &format!("{}/messages", live.item),
            Some(&live.f.token),
            &json!({"operation_id":super::super::mint(),"message":"queued follow-up"}).to_string(),
        )
        .await;
        assert_eq!(response.status(), http::StatusCode::ACCEPTED);
    }
    for sequence in 2..=26 {
        let accepted = frame(&mut body).await;
        assert!(accepted.contains("event: message.accepted\n"), "{accepted}");
        assert_eq!(data(&accepted)["sequence"], sequence.to_string());
    }
    let expected = live.delta("the running answer survives a deep queue");
    live.publish(&expected);
    let received = frame(&mut body).await;
    assert!(received.contains("event: message.delta\n"), "{received}");
    assert_eq!(data(&received), expected);
    sqlx::query("DELETE FROM identity.users WHERE id=$1::uuid")
        .bind(live.f.owner.as_str())
        .execute(&mut *live.f.db.acquire().await.expect("revocation connection"))
        .await
        .expect("revoke the person after their deep-queue preview");
    live.publish(&live.delta("revoked text stays private"));
    closed(&mut body).await;
    drop(body);
    live.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn malformed_unbounded_foreign_and_nonrunning_deltas_are_withheld() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    for raw in ["not-json".to_owned(), "x".repeat(32769)] {
        live.server.publish(&live.channel(), &raw);
    }
    for (field, invalid) in [
        ("draft_id", json!(super::super::mint())),
        ("sequence", json!("0")),
        ("sequence", json!("2")),
        ("sequence", json!("-1")),
        ("sequence", json!("9223372036854775808")),
        ("stream_seq", json!(0)),
        ("text_kind", json!("reasoning")),
        ("text", json!("")),
        ("text", json!("x".repeat(4097))),
    ] {
        let mut invalid_frame = live.delta("must stay private");
        invalid_frame[field] = invalid;
        live.publish(&invalid_frame);
    }
    let marker = live.delta("valid after rejected pushes");
    live.publish(&marker);
    assert_eq!(data(&frame(&mut body).await), marker);
    drop(body);
    live.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn stopped_message_preview_is_withheld_before_a_new_running_message() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    live.mutate("UPDATE authoring.messages SET status='cancelled' WHERE draft_id=$1::uuid")
        .await;
    live.mutate("UPDATE authoring.drafts SET cancelled_through_sequence=1 WHERE id=$1::uuid")
        .await;
    live.publish(&live.delta("stopped private text"));
    let cancelled = frame(&mut body).await;
    assert!(
        cancelled.contains("event: message.cancelled\n"),
        "{cancelled}"
    );
    assert!(!cancelled.contains("stopped private text"));
    let response = crate::harness::send(
        &live.router,
        http::Method::POST,
        &format!("{}/messages", live.item),
        Some(&live.f.token),
        &json!({"operation_id":super::super::mint(),"message":"continue"}).to_string(),
    )
    .await;
    assert_eq!(response.status(), http::StatusCode::ACCEPTED);
    live.mutate(
        "UPDATE authoring.messages SET status='running' WHERE draft_id=$1::uuid AND sequence=2",
    )
    .await;
    assert!(frame(&mut body).await.contains("event: message.running\n"));
    let mut marker = live.delta("new message answer");
    marker["sequence"] = Value::String("2".into());
    live.publish(&marker);
    assert_eq!(data(&frame(&mut body).await), marker);
    drop(body);
    live.cleanup().await;
}
