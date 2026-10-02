//! A member of an account, over live Postgres and Dragonfly: what they reach in
//! the owner's workspace, what they are refused, and how an open stream ends
//! when the owner removes them.
//!
//! The access decision is the production resolver reading real membership
//! rows, so these cases prove the role comes from the database. Where each
//! refusal lands across every route is `workspace_member_roles.rs`'s claim.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

#[path = "support/workspace_members_fixture.rs"]
pub(crate) mod fixture;

use std::time::Duration;

use afd_core::error_code;
use afd_sse::KIND_ACCESS_REVOKED;
use afd_tenant::workspace::access::{ROLE_MEMBER, ROLE_OWNER};
use axum::Router;
use axum::body::BodyDataStream;
use http::{Method, StatusCode};
use serde_json::Value;

use self::fixture::{Members, Person, owner_scopes, platform_scopes};
use crate::harness::{self, exchange, items, send};
use crate::integration_fleet_streams::fixture::{next_chunk, stream_ends};

const LIST: &str = "/v1/tenants/me/workspaces";
const STEER: &str = r#"{"message":"ship the next change"}"#;

fn fleets_of(person: &Person) -> String {
    format!("/v1/workspaces/{}/fleets", person.workspace.as_str())
}

/// The listed item for `workspace`, when the list carries it.
fn item<'list>(list: &'list Value, workspace: &Person) -> Option<&'list Value> {
    items(list)
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(workspace.workspace.as_str()))
}

async fn get(router: &Router, path: &str, who: &Person) -> (StatusCode, Value) {
    exchange(router, Method::GET, path, Some(&who.token), "").await
}

async fn open(router: &Router, path: &str, who: &Person) -> BodyDataStream {
    let response = send(router, Method::GET, path, Some(&who.token), "").await;
    assert_eq!(response.status(), StatusCode::OK, "{path} opens");
    response.into_body().into_data_stream()
}

/// The next event that is not a heartbeat, or the third heartbeat running.
///
/// Skipping the paused clock past both beats makes the heartbeat due while the
/// re-check is still reading Postgres, so a heartbeat can arrive first. It says
/// nothing about access; the frame after it does. Three in a row are returned
/// as they are, for the caller's assertion to name.
async fn past_heartbeats(body: &mut BodyDataStream) -> String {
    let mut event = next_chunk(body).await;
    for _beat in 0..2 {
        if !event.contains("event: heartbeat") {
            break;
        }
        event = next_chunk(body).await;
    }
    event
}

/// Dimension 2.1: Bob opens, lists, streams and steers John's workspace, and
/// the list names the account it is in and Bob's role there.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_member_reaches_owner_workspace() {
    let members = Members::create().await;
    let (router, hub) = members.live(&members.bob, owner_scopes()).await;
    let (john, bob) = (&members.john, &members.bob);

    let (status, list) = get(&router, LIST, bob).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(
        list.get("tenant_id").and_then(Value::as_str),
        Some(bob.tenant.as_str())
    );
    let johns = item(&list, john).expect("John's workspace is listed for Bob");
    assert_eq!(
        johns.pointer("/account/tenant_id").and_then(Value::as_str),
        Some(john.tenant.as_str())
    );
    assert_eq!(
        johns.pointer("/account/owner_name").and_then(Value::as_str),
        Some(john.display_name)
    );
    assert_eq!(johns.get("role").and_then(Value::as_str), Some(ROLE_MEMBER));
    let own = item(&list, bob).expect("Bob's own workspace is still listed");
    assert_eq!(own.get("role").and_then(Value::as_str), Some(ROLE_OWNER));

    let (status, fleets) = get(&router, &fleets_of(john), bob).await;
    assert_eq!(status, StatusCode::OK, "{fleets}");

    let steered = send(
        &router,
        Method::POST,
        &members.thread(),
        Some(&bob.token),
        STEER,
    )
    .await;
    assert_eq!(
        steered.status(),
        StatusCode::ACCEPTED,
        "a member steers the owner's fleet"
    );

    let stream = format!("/v1/workspaces/{}/events/stream", john.workspace.as_str());
    let mut body = open(&router, &stream, bob).await;
    assert!(next_chunk(&mut body).await.contains("event: hello"));
    drop(body);

    hub.shutdown();
    members.cleanup().await;
}

/// Dimension 2.2: an owner of another account, holding everything signup
/// grants and no membership, is refused John's workspace with exactly today's
/// answer. Not every scope: the platform-wide one is how an operator crosses,
/// and that crossing is §5's to prove.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_non_member_refused_unchanged() {
    let members = Members::create().await;
    let router = members.router(&members.stranger, owner_scopes());

    let (status, refused) = get(&router, &fleets_of(&members.john), &members.stranger).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        harness::error_code(&refused),
        Some(error_code::AUTH_FORBIDDEN.as_str())
    );
    assert_eq!(
        refused.get("detail").and_then(Value::as_str),
        Some("Workspace access denied")
    );
    assert_eq!(
        refused.get("title").and_then(Value::as_str),
        Some("Forbidden")
    );
    members.cleanup().await;
}

/// Dimension 2.3, from real rows: Bob reads John's secrets and is refused
/// writing one, with the owner-only code rather than a foreign-workspace one.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_member_refused_owner_only_routes() {
    let members = Members::create().await;
    let router = members.router(&members.bob, owner_scopes());
    let secrets = format!("/v1/workspaces/{}/secrets", members.john.workspace.as_str());

    let (status, listed) = get(&router, &secrets, &members.bob).await;
    assert_eq!(status, StatusCode::OK, "a member reads the names: {listed}");

    let (status, refused) = exchange(
        &router,
        Method::PUT,
        &format!("{secrets}/FIXTURE"),
        Some(&members.bob.token),
        r#"{"value":"x"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
    assert_eq!(
        harness::error_code(&refused),
        Some(error_code::AUTH_OWNER_ONLY.as_str())
    );
    members.cleanup().await;
}

/// Dimension 2.4: John removes Bob while Bob watches, and each of Bob's open
/// streams ends on `access_revoked` within one re-check.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_removed_member_stream_ends() {
    let members = Members::create().await;
    let (router, hub) = members.live(&members.bob, owner_scopes()).await;
    let wall = format!(
        "/v1/workspaces/{}/events/stream",
        members.john.workspace.as_str()
    );
    let mut wall_body = open(&router, &wall, &members.bob).await;
    assert!(next_chunk(&mut wall_body).await.contains("event: hello"));
    let mut tail_body = open(&router, &members.tail(), &members.bob).await;
    assert!(next_chunk(&mut tail_body).await.contains("event: hello"));

    members.remove_bob().await;
    // Past both beats on the paused clock, then real time again: each
    // re-check reads Postgres, and a paused runtime idling on a socket would
    // auto-advance into the read's own deadline.
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(16)).await;
    tokio::time::resume();

    for body in [&mut wall_body, &mut tail_body] {
        let last = past_heartbeats(body).await;
        assert!(
            last.contains(&format!("event: {KIND_ACCESS_REVOKED}")),
            "{last}"
        );
        assert!(last.contains(error_code::AUTH_FORBIDDEN.as_str()), "{last}");
        assert!(stream_ends(body).await, "nothing follows access_revoked");
    }
    hub.shutdown();
    members.cleanup().await;
}

/// Dimension 2.6: an owner with no invites sees exactly their own account,
/// and steers as before.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_single_owner_paths_unchanged() {
    let members = Members::create().await;
    let (router, hub) = members.live(&members.john, owner_scopes()).await;
    let john = &members.john;

    let (status, list) = get(&router, LIST, john).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let listed = items(&list);
    assert_eq!(
        listed.len(),
        1,
        "John holds one account with one workspace: {list}"
    );
    let only = listed.first().expect("the one workspace");
    assert_eq!(only.get("role").and_then(Value::as_str), Some(ROLE_OWNER));
    assert_eq!(
        only.pointer("/account/tenant_id").and_then(Value::as_str),
        Some(john.tenant.as_str())
    );

    let (status, _fleets) = get(&router, &fleets_of(john), john).await;
    assert_eq!(status, StatusCode::OK);
    let steered = send(
        &router,
        Method::POST,
        &members.thread(),
        Some(&john.token),
        STEER,
    )
    .await;
    assert_eq!(steered.status(), StatusCode::ACCEPTED);

    hub.shutdown();
    members.cleanup().await;
}

/// An operator holding the platform-wide scope, a stranger to John's account,
/// steers John's fleet: the crossing admits the write, and the admitted
/// message names the operator rather than the owner whose fleet it is.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_platform_write_acts_attributed() {
    let members = Members::create().await;
    let operator = &members.stranger;
    let (router, hub) = members.live(operator, platform_scopes()).await;

    let steered = send(
        &router,
        Method::POST,
        &members.thread(),
        Some(&operator.token),
        STEER,
    )
    .await;
    assert_eq!(
        steered.status(),
        StatusCode::ACCEPTED,
        "the platform scope crosses to write"
    );
    assert_eq!(
        members.admitted_actors().await,
        [operator.actor()],
        "one admission, attributed to the operator"
    );

    hub.shutdown();
    members.cleanup().await;
}
