//! A person's message, announced on the fleet's live tail the moment it is
//! accepted, over live Postgres and Dragonfly.
//!
//! Bob, a member of John's account, steers John's fleet while John watches
//! it: John's open stream carries `event_admitted` before any runner has the
//! message, under the id Bob's 202 names. A repeat announces nothing, and a
//! queue that will not take the frame costs the steer nothing.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "integration preconditions should fail the test loudly; a frame's JSON is read by key"
)]

use std::time::Duration;

use afd_core::test_util::trace::Capture;
use afd_dragonfly::{Dragonfly, SubscriptionHub};
use axum::Router;
use axum::body::BodyDataStream;
use futures_util::StreamExt as _;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::harness::{self, exchange, items, send};
use crate::integration_fleet_streams::fixture::{data_of, next_chunk};
use crate::integration_workspace_members::fixture::{Members, owner_scopes};

/// What Bob types.
const MESSAGE: &str = "check the tests";

/// The frame's `event:` line, as the SSE layer names it from the payload.
const ADMITTED_EVENT: &str = "event: event_admitted";

/// The publisher's event for a frame the queue would not take.
const EVENT_FRAME_DROPPED: &str = "tail_frame_dropped";

/// How long a stream must stay quiet to show nothing was published: well
/// under the fifteen-second heartbeat, well over a local publish.
const QUIET: Duration = Duration::from_millis(750);

/// John's fleet tail, opened as John and read past its greeting, and the hub
/// it streams through.
async fn watch(members: &Members) -> (BodyDataStream, SubscriptionHub) {
    let (router, hub) = members.live(&members.john, owner_scopes()).await;
    let path = members.tail();
    let response = send(&router, Method::GET, &path, Some(&members.john.token), "").await;
    assert_eq!(response.status(), StatusCode::OK, "{path} opens");
    let mut body = response.into_body().into_data_stream();
    assert!(next_chunk(&mut body).await.contains("event: hello"));
    (body, hub)
}

/// Bob's steer, answered: the status and the event id the 202 names.
async fn steer(router: &Router, members: &Members, body: &Value) -> (StatusCode, Value) {
    exchange(
        router,
        Method::POST,
        &members.thread(),
        Some(&members.bob.token),
        &body.to_string(),
    )
    .await
}

/// Whether `body` stays silent for [`QUIET`].
async fn stays_quiet(body: &mut BodyDataStream) -> bool {
    tokio::time::timeout(QUIET, body.next()).await.is_err()
}

fn bob_router(members: &Members, queue: Dragonfly, hub: &SubscriptionHub) -> Router {
    members.live_router(&members.bob, owner_scopes(), queue, hub.clone())
}

/// Dimension 1.1: the watcher sees the typed message under the 202's id.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_steer_publishes_admitted_frame() {
    let members = Members::create().await;
    let (mut johns, hub) = watch(&members).await;
    let bob = bob_router(&members, harness::connect_redis().await, &hub);

    let (status, accepted) = steer(&bob, &members, &json!({ "message": MESSAGE })).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");

    let event = next_chunk(&mut johns).await;
    assert!(event.contains(ADMITTED_EVENT), "{event}");
    let frame = data_of(&event);
    assert_eq!(
        frame["event_id"], accepted["event_id"],
        "one id, frame and 202"
    );
    assert_eq!(frame["message"], json!(MESSAGE));
    assert_eq!(frame["actor"], json!(members.bob.actor()));
    assert!(
        frame["created_at"].as_i64().is_some_and(|at| at > 0),
        "{frame}"
    );

    hub.shutdown();
    members.cleanup().await;
}

/// Dimension 1.3: a repeat of one operation is answered, never announced again.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_replayed_steer_publishes_nothing() {
    let members = Members::create().await;
    let (mut johns, hub) = watch(&members).await;
    let bob = bob_router(&members, harness::connect_redis().await, &hub);
    let body = json!({ "message": MESSAGE, "operation_id": "send-once" });

    let (status, first) = steer(&bob, &members, &body).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{first}");
    assert!(next_chunk(&mut johns).await.contains(ADMITTED_EVENT));

    let (status, again) = steer(&bob, &members, &body).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{again}");
    assert_eq!(again["replayed"], json!(true));
    assert_eq!(again["event_id"], first["event_id"]);
    assert!(stays_quiet(&mut johns).await, "a repeat announces nothing");

    hub.shutdown();
    members.cleanup().await;
}

/// Dimension 1.2: a queue that refuses the frame costs the steer nothing, and
/// the publisher's one log line never carries what was typed.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_admitted_publish_failure_still_accepts() {
    let members = Members::create().await;
    let hub = harness::live_hub().await;
    let queue = Dragonfly::unreachable(&harness::unreachable_queue())
        .expect("a lazy manager opens no socket, so it cannot fail to open one");
    let bob = bob_router(&members, queue, &hub);

    let log = Capture::install();
    let (status, accepted) = steer(&bob, &members, &json!({ "message": MESSAGE })).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    let dropped = log.only(EVENT_FRAME_DROPPED);
    assert_eq!(dropped.field("fleet_id"), Some(members.fleet.as_str()));
    assert!(
        log.events()
            .iter()
            .all(|event| event.fields.values().all(|value| !value.contains(MESSAGE))),
        "no log line carries the typed message"
    );
    drop(log);

    hub.shutdown();
    members.cleanup().await;
}

/// Dimension 4.3: a workspace's member list names each member by the actor
/// their messages record, so a thread can put a name on each one.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_workspace_members_carry_actor() {
    let members = Members::create().await;
    let (bob, hub) = members.live(&members.bob, owner_scopes()).await;
    let path = format!("/v1/workspaces/{}/members", members.john.workspace.as_str());

    let (status, list) = exchange(&bob, Method::GET, &path, Some(&members.bob.token), "").await;
    assert_eq!(status, StatusCode::OK);
    let item = items(&list)
        .iter()
        .find(|item| item["user_id"] == json!(members.bob.user))
        .expect("Bob is listed in John's workspace");
    assert_eq!(item["actor"], json!(members.bob.actor()));
    assert!(
        item.get("email").is_none(),
        "no address on this list: {item}"
    );

    hub.shutdown();
    members.cleanup().await;
}
