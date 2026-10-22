//! Transport loss retains durable reads and releases bounded stream admission.

use super::live::{Live, closed, data, frame};
use futures_util::StreamExt as _;
use http::StatusCode;
use serde_json::json;
use sqlx::Connection as _;
use std::time::Duration;

const BLOCKED: &str =
    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_blocked_authorization_read_closes_at_its_deadline_and_releases_capacity() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    let mut owner = sqlx::PgConnection::connect(&live.f.owner_url())
        .await
        .expect("lock owner");
    let mut held = owner.begin().await.expect("hold draft");
    let pid = lock_draft(&mut held, &live).await;
    let started = tokio::time::Instant::now();
    let read = tokio::spawn(async move {
        let next = body.next().await;
        (next, body)
    });
    let mut monitor = sqlx::PgConnection::connect(&live.f.owner_url())
        .await
        .expect("autocommit monitor");
    wait_for_block(&mut monitor, pid).await;
    let (next, body) = tokio::time::timeout(Duration::from_secs(12), read)
        .await
        .expect("bounded access read")
        .expect("stream task");
    assert!(
        next.is_none(),
        "blocked authorization cannot emit buffered text"
    );
    assert!(started.elapsed() >= Duration::from_secs(10));
    held.rollback().await.expect("release draft");
    drop(body);
    assert_eq!(live.hub.readers(&live.channel()), 0);
    let replacement = live.open().await;
    drop(replacement);
    monitor.close().await.expect("release monitor");
    owner.close().await.expect("release owner");
    live.cleanup().await;
}

async fn lock_draft(held: &mut sqlx::Transaction<'_, sqlx::Postgres>, live: &Live) -> i32 {
    sqlx::query_scalar("SELECT pg_backend_pid() FROM authoring.drafts WHERE id=$1::uuid FOR UPDATE")
        .bind(live.draft.as_str())
        .fetch_one(&mut **held)
        .await
        .expect("draft row lock")
}

async fn wait_for_block(monitor: &mut sqlx::PgConnection, pid: i32) {
    tokio::time::timeout(super::DELIVERY, async {
        while !sqlx::query_scalar::<_, bool>(BLOCKED)
            .bind(pid)
            .fetch_one(&mut *monitor)
            .await
            .expect("current lock census")
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("authorization actually waits on the held draft");
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn subscription_gap_and_lag_request_resync_without_private_text() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    live.server.confirm(&live.channel());
    let gap = frame(&mut body).await;
    assert!(gap.contains("event: draft.resync\n"), "{gap}");
    assert_eq!(data(&gap), json!({}));
    for _ in 0..300 {
        live.publish(&live.delta("buffer overflow private answer"));
    }
    let lag = frame(&mut body).await;
    assert!(lag.contains("event: draft.resync\n"), "{lag}");
    assert_eq!(data(&lag), json!({}));
    drop(body);
    assert_eq!(live.hub.readers(&live.channel()), 0);
    live.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn stopped_hub_falls_back_to_durable_notices_then_closes() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    live.hub.shutdown();
    let heartbeat =
        tokio::time::timeout(afd_sse::HEARTBEAT_INTERVAL + super::DELIVERY, body.next())
            .await
            .expect("a stopped hub retains periodic durable reads")
            .expect("the running draft stays open")
            .expect("infallible body");
    assert_eq!(
        heartbeat.as_ref(),
        b"event: heartbeat\ndata: {\"kind\":\"heartbeat\"}\n\n"
    );
    live.mutate("UPDATE authoring.messages SET status='completed' WHERE draft_id=$1::uuid")
        .await;
    live.mutate("UPDATE authoring.drafts SET status='closed',closed_at=now(),expires_at=now()+interval '1 day' WHERE id=$1::uuid").await;
    assert!(
        frame(&mut body)
            .await
            .contains("event: message.completed\n")
    );
    assert!(frame(&mut body).await.contains("event: draft.closed\n"));
    closed(&mut body).await;
    drop(body);
    live.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn closed_draft_never_emits_a_buffered_private_preview() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    live.mutate("UPDATE authoring.drafts SET status='closed',closed_at=now(),expires_at=now()+interval '1 day' WHERE id=$1::uuid").await;
    live.publish(&live.delta("late private text"));
    let last = frame(&mut body).await;
    assert!(last.contains("event: draft.closed\n"), "{last}");
    closed(&mut body).await;
    drop(body);
    live.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn stream_ceiling_refuses_excess_viewers_and_dropped_body_releases_its_slot() {
    let live = Live::create(1).await;
    let body = live.open().await;
    let refused = live.response().await;
    assert_eq!(refused.status(), StatusCode::SERVICE_UNAVAILABLE);
    let problem = crate::harness::json_body(refused).await;
    assert_eq!(
        problem["error_code"],
        afd_core::error_code::SSE_STREAM_CAP.as_str()
    );
    assert_eq!(live.hub.readers(&live.channel()), 1);
    drop(body);
    let replacement = live.open().await;
    drop(replacement);
    assert_eq!(live.hub.readers(&live.channel()), 0);
    live.cleanup().await;
}
