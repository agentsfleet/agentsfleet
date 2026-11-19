//! A broker barrier observes restart hints only after an execution row is issued.

use super::chat::Chat;
use af_core::clock;
use af_wire::event::EventType;
use afd_bus::{Received, Subject, Subscription};
use afd_fleet::run::Kind;
use futures_util::future::join_all;
use serde_json::Value;
use std::time::Duration;

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fresh_issue_and_unissued_reservation_publish_no_restart() -> Checked {
    let chat = Chat::open().await;
    chat.ask(EventType::Chat, "Restore saved progress", clock::now())
        .await;
    let mut reader = chat
        .fixtures
        .bus()
        .subscribe(&Subject::FleetTail(chat.fleet().clone()))
        .await?;
    chat.run(&chat.runner, clock::now()).await;
    assert_eq!(restarts(&chat, &mut reader).await?, [] as [Value; 0]);
    expire(&chat).await?;
    let claimed = chat
        .claim(&chat.spare)
        .await
        .ok_or("replacement reservation")?;
    assert_eq!(claimed.kind, Kind::Reclaim);
    assert_eq!(restarts(&chat, &mut reader).await?, [] as [Value; 0]);
    assert_eq!(issued(&chat).await?, 1);
    drop(reader);
    chat.fixtures.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn one_hundred_contending_claimers_publish_one_committed_restart() -> Checked {
    let chat = Chat::open().await;
    let event = chat
        .ask(EventType::Chat, "Resume interrupted work", clock::now())
        .await;
    chat.run(&chat.runner, clock::now()).await;
    expire(&chat).await?;
    let mut reader = chat
        .fixtures
        .bus()
        .subscribe(&Subject::FleetTail(chat.fleet().clone()))
        .await?;
    let claims: Vec<_> = join_all((0..100).map(|_| chat.claim(&chat.spare)))
        .await
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(claims.len(), 1);
    let selected = claims
        .into_iter()
        .next()
        .ok_or("one replacement reservation")?;
    assert_eq!(selected.kind, Kind::Reclaim);
    assert_eq!(restarts(&chat, &mut reader).await?, [] as [Value; 0]);
    let reply = chat
        .fixtures
        .plane()
        .claim_selected(selected.clone(), &chat.spare, clock::now())
        .await?;
    assert!(reply.is_some());
    let frames = restarts(&chat, &mut reader).await?;
    assert_eq!(frames.len(), 1);
    let frame = frames.first().ok_or("one restart")?;
    assert_eq!(frame["event_id"], event.as_str());
    assert_eq!(frame["fencing_token"], selected.fence.as_i64().to_string());
    let created: jiff_sqlx::Timestamp = sqlx::query_scalar(
        "SELECT created_at FROM thread.runs WHERE fleet_id=$1 AND fencing_token=$2",
    )
    .bind(chat.fleet())
    .bind(selected.fence.as_i64())
    .fetch_one(&mut *chat.fixtures.database.acquire().await?)
    .await?;
    assert_eq!(frame["restarted_at"], created.to_jiff().to_string());
    assert_eq!(issued(&chat).await?, 2);
    drop(reader);
    chat.fixtures.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_issue_refused_after_reservation_publishes_no_restart() -> Checked {
    let chat = Chat::open().await;
    chat.ask(EventType::Chat, "Wait for a healthy host", clock::now())
        .await;
    chat.run(&chat.runner, clock::now()).await;
    expire(&chat).await?;
    let selected = chat.claim(&chat.spare).await.ok_or("replacement claim")?;
    assert_eq!(selected.kind, Kind::Reclaim);
    sqlx::query(
        "UPDATE runner.runners SET cordon_reason=$2,cordoned_at=clock_timestamp() WHERE id=$1",
    )
    .bind(&chat.spare)
    .bind(afd_api_wire::operator::CordonReason::MissedHeartbeats.as_str())
    .execute(&mut *chat.fixtures.database.acquire().await?)
    .await?;
    let mut reader = chat
        .fixtures
        .bus()
        .subscribe(&Subject::FleetTail(chat.fleet().clone()))
        .await?;
    let answer = chat
        .fixtures
        .plane()
        .claim_selected(selected, &chat.spare, clock::now())
        .await?;
    assert!(answer.is_none());
    assert_eq!(restarts(&chat, &mut reader).await?, [] as [Value; 0]);
    assert_eq!(issued(&chat).await?, 1);
    drop(reader);
    chat.fixtures.cleanup().await;
    Ok(())
}

async fn expire(chat: &Chat) -> Checked {
    let mut connection = chat.fixtures.database.acquire().await?;
    sqlx::query("UPDATE thread.runs SET run_expires_at=clock_timestamp()-interval '1 second' WHERE fleet_id=$1")
        .bind(chat.fleet()).execute(&mut *connection).await?;
    sqlx::query("UPDATE runner.slots SET run_expires_at=clock_timestamp()-interval '1 second' WHERE fleet_id=$1")
        .bind(chat.fleet()).execute(&mut *connection).await?;
    Ok(())
}

async fn issued(chat: &Chat) -> Checked<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM thread.runs WHERE fleet_id=$1")
            .bind(chat.fleet())
            .fetch_one(&mut *chat.fixtures.database.acquire().await?)
            .await?,
    )
}

async fn restarts(chat: &Chat, reader: &mut Subscription) -> Checked<Vec<Value>> {
    let barrier = serde_json::json!({"test_barrier":afd_db::test_util::mint_id()}).to_string();
    chat.fixtures
        .bus()
        .publish(&Subject::FleetTail(chat.fleet().clone()), &barrier)
        .await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut frames = Vec::new();
        loop {
            let Received::Message(message) = reader.recv().await? else {
                return Err("restart observation lost broker traffic".into());
            };
            if message.payload.as_str() == barrier {
                return Ok(frames);
            }
            let value: Value = serde_json::from_str(&message.payload)?;
            if value.get("kind").and_then(Value::as_str) == Some("run_restarted") {
                frames.push(value);
            }
        }
    })
    .await?
}
