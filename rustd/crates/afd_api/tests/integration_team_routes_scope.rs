//! The team routes never reach past the caller's own account, over live
//! Postgres: an owner naming another account's invite or member changes
//! nothing, and a member's `/tenants/me` routes read their own account.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::error_code;
use http::{Method, StatusCode};
use serde_json::json;

use crate::harness;
use crate::integration_team_routes::{INVITES, MEMBERS, call, find, send_again, text};
use crate::integration_workspace_members::fixture::{Members, owner_scopes};

/// An owner naming another account's invite or member through their own
/// `/tenants/me` routes reaches nothing: the revoke and the removal are quiet
/// no-ops, the send is not found and counts no attempt, and John's invite and
/// Bob's membership stand.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_owner_routes_never_reach_another_account() {
    let members = Members::create().await;
    let (john, bob, stranger) = (&members.john, &members.bob, &members.stranger);
    let johns = members.router(john, owner_scopes());
    let strangers = members.router(stranger, owner_scopes());
    let body = json!({ "email": format!("dave+{}@example.test", bob.user) }).to_string();
    let (status, invite) = call(&johns, Method::POST, INVITES, john, &body).await;
    assert_eq!(status, StatusCode::CREATED, "{invite}");
    let invite = text(&invite, "id").expect("an id").to_owned();
    let counted = members.email_attempts(&invite).await;

    let revoke = format!("{INVITES}/{invite}");
    let (status, _) = call(&strangers, Method::DELETE, &revoke, stranger, "").await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "nothing of theirs to revoke"
    );
    let (status, problem) = send_again(&strangers, stranger, &invite).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    assert_eq!(
        harness::error_code(&problem),
        Some(error_code::INVITE_NOT_FOUND.as_str())
    );
    assert_eq!(
        members.email_attempts(&invite).await,
        counted,
        "no send counted"
    );
    let remove = format!("{MEMBERS}/{}", bob.user);
    let (status, _) = call(&strangers, Method::DELETE, &remove, stranger, "").await;
    assert_eq!(status, StatusCode::NO_CONTENT, "nobody of theirs to remove");

    let (_, listed) = call(&johns, Method::GET, INVITES, john, "").await;
    assert!(
        find(&listed, "id", &invite).is_some(),
        "John's invite stands: {listed}"
    );
    let (_, roster) = call(&johns, Method::GET, MEMBERS, john, "").await;
    assert!(
        find(&roster, "user_id", &bob.user).is_some(),
        "Bob stays: {roster}"
    );
    members.cleanup().await;
}

/// Bob is a member of John's account and owns his own: his `/tenants/me`
/// routes read his own account, never John's invites or roster.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_member_manages_only_own_account() {
    let members = Members::create().await;
    let (john, bob, stranger) = (&members.john, &members.bob, &members.stranger);
    let johns = members.router(john, owner_scopes());
    let bobs = members.router(bob, owner_scopes());
    let body = json!({ "email": stranger.email }).to_string();
    let (status, invite) = call(&johns, Method::POST, INVITES, john, &body).await;
    assert_eq!(status, StatusCode::CREATED, "{invite}");
    let invite = text(&invite, "id").expect("an id").to_owned();

    let (status, listed) = call(&bobs, Method::GET, INVITES, bob, "").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert!(
        find(&listed, "id", &invite).is_none(),
        "not Bob's invite: {listed}"
    );
    let (status, roster) = call(&bobs, Method::GET, MEMBERS, bob, "").await;
    assert_eq!(status, StatusCode::OK, "{roster}");
    assert!(
        find(&roster, "user_id", &john.user).is_none(),
        "not Bob's roster: {roster}"
    );
    assert!(
        find(&roster, "user_id", &bob.user).is_some(),
        "Bob owns his own: {roster}"
    );
    members.cleanup().await;
}
