//! Content-free summary hints obey the same fresh private-stream checks as answers.
use super::live::{Live, closed, data, frame};
use serde_json::{Value, json};

fn notice(live: &Live, state: &str) -> Value {
    json!({"draft_id":live.draft,"sequence":"1","stream_id":super::super::mint(),
        "progress":{"generation":1,"state":state}})
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn private_summary_status_arrives_incrementally_without_changing_saved_history() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    for state in ["summarizing", "resuming", "interrupted"] {
        let expected = notice(&live, state);
        live.publish(&expected);
        let received = frame(&mut body).await;
        assert!(received.contains("event: context.progress\n"), "{received}");
        assert_eq!(data(&received), expected);
    }
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
    assert!(!messages.to_string().contains("summarizing"));
    drop(body);
    assert_eq!(live.hub.readers(&live.channel()), 0);
    live.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn malformed_foreign_and_nonrunning_summary_notices_never_reach_the_private_reader() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    for (field, value) in [
        ("draft_id", json!(super::super::mint())),
        ("sequence", json!("0")),
        ("sequence", json!("2")),
        ("sequence", json!("9223372036854775808")),
        ("progress", json!({"generation":0,"state":"summarizing"})),
        ("progress", json!({"generation":1,"state":"unknown"})),
        (
            "progress",
            json!({"generation":1,"state":"summarizing","notes":"private notes"}),
        ),
        ("notes", json!("private notes")),
    ] {
        let mut invalid = notice(&live, "summarizing");
        invalid[field] = value;
        live.publish(&invalid);
    }
    let marker = live.delta("authorized answer after rejected summary hints");
    live.publish(&marker);
    assert_eq!(data(&frame(&mut body).await), marker);
    drop(body);
    live.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn stopping_a_message_withholds_its_buffered_summary_notice() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    live.mutate("UPDATE authoring.messages SET status='cancelled' WHERE draft_id=$1::uuid")
        .await;
    live.mutate("UPDATE authoring.drafts SET cancelled_through_sequence=1 WHERE id=$1::uuid")
        .await;
    live.publish(&notice(&live, "resuming"));
    let received = frame(&mut body).await;
    assert!(
        received.contains("event: message.cancelled\n"),
        "{received}"
    );
    assert!(!received.contains("resuming"));
    live.server.publish(&live.channel(), "{\"invalid\":true}");
    drop(body);
    live.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn revocation_withholds_a_previously_authorized_summary_ending() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    live.publish(&notice(&live, "summarizing"));
    assert_eq!(
        data(&frame(&mut body).await)["progress"]["state"],
        "summarizing"
    );
    sqlx::query("DELETE FROM identity.users WHERE id=$1::uuid")
        .bind(live.f.owner.as_str())
        .execute(&mut *live.f.db.acquire().await.expect("revocation connection"))
        .await
        .expect("revoke only this fixture's person");
    live.publish(&notice(&live, "resuming"));
    closed(&mut body).await;
    drop(body);
    live.cleanup().await;
}
