//! Transport loss retains durable reads and releases bounded stream admission.

use super::live::{Live, closed, data, frame};
use afd_bus::{Bus, Handle, Subject, Subscription};
use futures_util::StreamExt as _;
use http::StatusCode;
use serde_json::json;
use sqlx::Connection as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// The stream reads its draft in a snapshot and takes no row lock, so a writer
/// that holds the draft row does not end it. It used to wait on that lock and
/// close when the login's `lock_timeout` (`afd_db::login::LOCK_TIMEOUT`) passed.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_writer_holding_the_draft_row_does_not_close_the_stream() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    let mut owner = sqlx::PgConnection::connect(&live.f.owner_url())
        .await
        .expect("lock owner");
    let mut held = owner.begin().await.expect("hold draft");
    sqlx::query("SELECT id FROM setup.drafts WHERE id=$1 FOR UPDATE")
        .bind(&live.draft)
        .fetch_one(&mut *held)
        .await
        .expect("draft row lock");
    // gstack-timing-ok: the delivery deadline outlasts the lock timeout that
    // used to close a waiting stream, so a still-open stream proves no wait.
    let waited = tokio::time::timeout(super::DELIVERY, body.next()).await;
    assert!(
        waited.is_err(),
        "an idle stream stays open while a writer holds its draft row"
    );
    held.rollback().await.expect("release draft");
    drop(body);
    assert_eq!(live.readers(), 0);
    owner.close().await.expect("release owner");
    live.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn subscription_gap_and_lag_request_resync_without_private_text() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    live.server.gap(&live.subject());
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
    assert_eq!(live.readers(), 0);
    live.cleanup().await;
}

/// A bus that shut down never announces again, so the stream ends — after at
/// most a resync — and the client reconnects to a replica that is serving. No
/// timer keeps reading Postgres for it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn draft_stream_ends_when_its_bus_shuts_down() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    live.bus.shutdown().await.expect("the bus shuts down");
    loop {
        let next = tokio::time::timeout(super::DELIVERY, body.next())
            .await
            .expect("a shut bus ends the stream promptly");
        let Some(chunk) = next else {
            break;
        };
        let chunk = chunk.expect("infallible body");
        assert_eq!(
            chunk.as_ref(),
            b"event: draft.resync\ndata: {}\n\n",
            "only a resync precedes the end"
        );
    }
    drop(body);
    assert_eq!(live.readers(), 0);
    live.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_second_loss_before_any_delivery_is_not_announced_again() {
    let live = Live::over(1, |inner| {
        Handle::new(Fragile {
            drops: AtomicUsize::new(2),
            inner,
        })
    })
    .await;
    let mut body = live.open().await;
    let resync = frame(&mut body).await;
    assert!(resync.contains("event: draft.resync\n"), "{resync}");
    // The resubscribe is lost again before it delivers anything; the third
    // subscription holds. Publish once it is attached.
    let expected = live.delta("after the outage");
    let (server, subject, payload) = (live.server.clone(), live.subject(), expected.to_string());
    let publisher = tokio::spawn(async move {
        while server.readers(&subject) == 0 {
            tokio::task::yield_now().await;
        }
        server.publish(&subject, &payload);
    });
    let delivered = frame(&mut body).await;
    assert!(
        delivered.contains("event: message.delta\n"),
        "one outage is announced once: {delivered}"
    );
    assert_eq!(data(&delivered), expected);
    publisher.await.expect("the publisher task");
    drop(body);
    assert_eq!(live.readers(), 0);
    live.cleanup().await;
}

/// A bus whose first `drops` subscriptions end at once with a transient
/// failure, then serves from `inner`.
#[derive(Debug)]
struct Fragile {
    drops: AtomicUsize,
    inner: Handle,
}

#[async_trait::async_trait]
impl Bus for Fragile {
    async fn publish(&self, subject: &Subject, payload: &str) -> afd_bus::Result<()> {
        self.inner.publish(subject, payload).await
    }

    async fn publish_batch(&self, subject: &Subject, payloads: &[String]) -> afd_bus::Result<()> {
        self.inner.publish_batch(subject, payloads).await
    }

    async fn subscribe(&self, subject: &Subject) -> afd_bus::Result<Subscription> {
        let dropping = self
            .drops
            .try_update(Ordering::AcqRel, Ordering::Acquire, |left| {
                left.checked_sub(1)
            })
            .is_ok();
        if !dropping {
            return self.inner.subscribe(subject).await;
        }
        // A transient failure, as a timed-out read is: not a closed bus.
        let Err(elapsed) = tokio::time::timeout(Duration::ZERO, std::future::pending::<()>()).await
        else {
            return self.inner.subscribe(subject).await;
        };
        Ok(Subscription::new(futures_util::stream::once(async move {
            Err(elapsed.into())
        })))
    }

    async fn subscribe_thread_wakes(&self) -> afd_bus::Result<Subscription> {
        self.inner.subscribe_thread_wakes().await
    }

    async fn shutdown(&self) -> afd_bus::Result<()> {
        self.inner.shutdown().await
    }

    async fn connected(&self) -> bool {
        self.inner.connected().await
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn closed_draft_never_emits_a_buffered_private_preview() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    live.mutate("UPDATE setup.drafts SET status='closed',closed_at=now(),expires_at=now()+interval '1 day' WHERE id=$1").await;
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
        af_core::error_code::SSE_STREAM_CAP.as_str()
    );
    assert_eq!(live.readers(), 1);
    drop(body);
    let replacement = live.open().await;
    drop(replacement);
    assert_eq!(live.readers(), 0);
    live.cleanup().await;
}

/// A bus that refuses the first `refusals` subscribes to a draft's items subject, then
/// serves them; every other subject passes through.
#[derive(Debug)]
struct Refusing {
    refusals: AtomicUsize,
    inner: Handle,
}

#[async_trait::async_trait]
impl Bus for Refusing {
    async fn publish(&self, subject: &Subject, payload: &str) -> afd_bus::Result<()> {
        self.inner.publish(subject, payload).await
    }

    async fn publish_batch(&self, subject: &Subject, payloads: &[String]) -> afd_bus::Result<()> {
        self.inner.publish_batch(subject, payloads).await
    }

    async fn subscribe(&self, subject: &Subject) -> afd_bus::Result<Subscription> {
        let refusing = matches!(subject, Subject::ThreadItems(_))
            && self
                .refusals
                .try_update(Ordering::AcqRel, Ordering::Acquire, |left| {
                    left.checked_sub(1)
                })
                .is_ok();
        if refusing {
            // A transient refusal, as a timed-out subscribe is: the link retries.
            if let Err(elapsed) =
                tokio::time::timeout(Duration::ZERO, std::future::pending::<()>()).await
            {
                return Err(elapsed.into());
            }
        }
        self.inner.subscribe(subject).await
    }

    async fn subscribe_thread_wakes(&self) -> afd_bus::Result<Subscription> {
        self.inner.subscribe_thread_wakes().await
    }

    async fn shutdown(&self) -> afd_bus::Result<()> {
        self.inner.shutdown().await
    }
}

/// A page whose subscription was refused at open keeps its heartbeat, reopens the
/// subscription on its retry without reading on the retry itself, and then hears the
/// next change on its notice.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_page_whose_subscription_was_refused_reopens_and_hears_the_next_change() {
    let live = Live::over(1, |inner| {
        Handle::new(Refusing {
            refusals: AtomicUsize::new(4),
            inner,
        })
    })
    .await;
    let mut body = live.open().await;
    // Four refusals back off 0.25 + 0.5 + 1 + 2 s; the page waits on each retry.
    tokio::time::sleep(Duration::from_secs(5)).await;
    live.mutate("UPDATE thread.turns SET status='completed' WHERE thread_id=$1")
        .await;
    live.publish(&json!({ "changed": live.draft }));
    let mut seen = Vec::new();
    loop {
        let chunk = frame(&mut body).await;
        let done = chunk.contains("event: message.completed\n");
        seen.push(chunk);
        if done {
            break;
        }
        assert!(seen.len() < 4, "the change arrives on its notice: {seen:?}");
    }
    drop(body);
    live.cleanup().await;
}
