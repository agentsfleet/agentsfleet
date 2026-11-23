//! The setup page reads on a notice, never on a timer: an idle page runs no
//! statement, a committed change reaches it on its notice, and a change whose
//! notice was lost shows at the next gap.

use std::time::Duration;

use afd_db::test_util::statements::{Window, counted_database};
use axum::body::BodyDataStream;
use futures_util::StreamExt as _;
use http::Method;
use serde_json::json;
use tokio::sync::mpsc;

use super::super::{fixture::Fixture, mint, send};
use super::live::{Live, frame};

/// Long enough for a notice to cross the in-process bus and the read it asks for.
const NOTICE: Duration = Duration::from_secs(2);

/// Long enough for the read after the opening frames, which finds nothing new and
/// settles the page, to finish.
const SETTLE: Duration = Duration::from_secs(1);

/// Every frame `body` sends, forwarded as text while the test moves the clock.
fn drain(mut body: BodyDataStream) -> mpsc::UnboundedReceiver<String> {
    let (frames, received) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(Ok(chunk)) = body.next().await {
            let text = String::from_utf8_lossy(&chunk).into_owned();
            if frames.send(text).is_err() {
                return;
            }
        }
    });
    received
}

fn is_heartbeat(frame: &str) -> bool {
    frame.contains(&format!("event: {}", afd_sse::HEARTBEAT_EVENT))
}

/// A minute of an open, idle page runs no statement: four heartbeats go out and
/// nothing is read.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_idle_draft_stream_reads_nothing() {
    let (lane, owner) = counted_database().await;
    let live = Live::on(Fixture::create_on(lane).await, 1, |bus| bus).await;
    let mut frames = drain(live.open().await);
    tokio::time::sleep(SETTLE).await;
    let window = Window::open(&owner).await;
    for _ in 0..60 {
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::time::resume();
        tokio::task::yield_now().await;
    }
    let ran = window.close().await;
    assert_eq!(
        ran.total(),
        0,
        "an idle page reads nothing: {:?}",
        ran.statements()
    );
    let mut heartbeats = 0;
    while let Ok(frame) = frames.try_recv() {
        assert!(is_heartbeat(&frame), "only heartbeats while idle: {frame}");
        heartbeats += 1;
    }
    assert!(heartbeats >= 3, "the heartbeat still runs: {heartbeats}");
    live.cleanup().await;
}

/// A message sent to the draft reaches its open page on the notice the send
/// publishes after its commit.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_draft_change_reaches_the_stream_on_its_notice() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    let sent = send(
        &live.router,
        Method::POST,
        &format!("{}/messages", live.item),
        Some(&live.f.token),
        &json!({"message": "Add a triage label.", "operation_id": mint()}).to_string(),
    )
    .await;
    assert!(
        sent.status().is_success(),
        "the send commits: {}",
        sent.status()
    );
    let changed = tokio::time::timeout(NOTICE, frame(&mut body))
        .await
        .expect("the change arrives on its notice");
    assert!(!is_heartbeat(&changed), "a state frame: {changed}");
    drop(body);
    live.cleanup().await;
}

/// A change committed with no notice — the writer died before publishing — is
/// not shown on a timer, and shows once a gap, such as a booting daemon's,
/// reaches the page.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_lost_notice_shows_at_the_next_gap() {
    let live = Live::create(1).await;
    let mut frames = drain(live.open().await);
    tokio::time::sleep(SETTLE).await;
    live.mutate("UPDATE thread.turns SET status='completed' WHERE thread_id=$1")
        .await;
    tokio::time::sleep(NOTICE).await;
    assert!(frames.try_recv().is_err(), "nothing shows without a notice");
    for _ in 0..16 {
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::time::resume();
        tokio::task::yield_now().await;
    }
    while let Ok(frame) = frames.try_recv() {
        assert!(is_heartbeat(&frame), "a heartbeat reads nothing: {frame}");
    }
    live.server.gap(&live.subject());
    let mut shown = Vec::new();
    while let Ok(Some(frame)) = tokio::time::timeout(NOTICE, frames.recv()).await {
        shown.push(frame);
    }
    assert!(
        shown
            .iter()
            .any(|frame| frame.contains("event: message.completed\n")),
        "the gap's read finds the change: {shown:?}"
    );
    live.cleanup().await;
}

/// A creator removed from the account loses their open page on the members notice
/// the removal publishes: the read it asks for no longer proves them, and the stream
/// ends.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_removed_creators_page_ends_on_the_members_notice() {
    let live = Live::create(1).await;
    let mut body = live.open().await;
    tokio::time::sleep(SETTLE).await;
    sqlx::query("DELETE FROM identity.users WHERE id=$1")
        .bind(&live.f.owner)
        .execute(&mut *live.f.db.acquire().await.expect("revocation connection"))
        .await
        .expect("remove the creator");
    live.server
        .publish(&afd_bus::Subject::Members(live.f.tenant.clone()), "");
    super::live::closed(&mut body).await;
    live.cleanup().await;
}
