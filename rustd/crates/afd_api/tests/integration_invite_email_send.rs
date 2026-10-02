//! Sending an invite's email again, and the team store failing under a send,
//! over live Postgres and a scripted relay: every refusal names its code, and
//! every case reads back what the invite row kept.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_mail::test_util::{FakeRelay, Session};
use afd_tenant::team::{EMAIL_STATUS_FAILED, EMAIL_STATUS_SENT};
use http::{Method, StatusCode};
use serde_json::json;

use crate::harness::{self, TeamStep, items, text};
use crate::integration_invite_email::{id_of, invite, key_header, listed_status, owner_fleet};
use crate::integration_team_routes::{INVITES, MEMBERS, call, send_again};
use crate::integration_workspace_members::fixture::Members;

/// Dimension 4.1: sending again after a failure is a new attempt under a new
/// key, and the invite flips to `sent`.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_send_again_after_failure() {
    let members = Members::create().await;
    let relay = FakeRelay::start(vec![Session::RefuseAuth(535), Session::Accept]).await;
    let router = owner_fleet(&members, Some(relay.port())).await.router();
    let (created, _address) = invite(&router, &members).await;
    let id = id_of(&created).to_owned();
    assert_eq!(text(&created, "email_status"), Some(EMAIL_STATUS_FAILED));

    let (status, answered) = send_again(&router, &members.john, &id).await;
    assert_eq!(status, StatusCode::OK, "{answered}");
    assert_eq!(text(&answered, "email_status"), Some(EMAIL_STATUS_SENT));
    let received = relay.received();
    assert_eq!(received.len(), 1);
    assert!(
        received
            .iter()
            .all(|message| message.contains(&key_header(&id, 2)))
    );
    assert_eq!(
        listed_status(&router, &members, &id).await,
        EMAIL_STATUS_SENT
    );
    members.cleanup().await;
}

/// Dimension 4.2: sending again with no relay is `503 UZ-INV-005`, and an
/// invite that is not pending is `404 UZ-INV-001`.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_send_again_unconfigured_refused() {
    let members = Members::create().await;
    let router = owner_fleet(&members, None).await.router();
    let (created, _address) = invite(&router, &members).await;
    let (status, problem) = send_again(&router, &members.john, id_of(&created)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        harness::error_code(&problem),
        Some(error_code::INVITE_EMAIL_UNAVAILABLE.as_str())
    );

    let unknown = Uuid7::parse(&mint_id()).expect("a minted id is canonical");
    let (status, problem) = send_again(&router, &members.john, unknown.as_str()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        harness::error_code(&problem),
        Some(error_code::INVITE_NOT_FOUND.as_str())
    );
    members.cleanup().await;
}

/// Sending again: a revoked invite is not found and counts nothing; a relay
/// that refuses is `503 UZ-INV-005` and the invite lists as `failed`.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_send_again_refusals_leave_the_row_true() {
    let members = Members::create().await;
    let relay = FakeRelay::start(vec![
        Session::RefuseRecipient(550),
        Session::RefuseRecipient(550),
        Session::RefuseRecipient(550),
    ])
    .await;
    let router = owner_fleet(&members, Some(relay.port())).await.router();

    let (refused, _address) = invite(&router, &members).await;
    let refused = id_of(&refused).to_owned();
    let (status, problem) = send_again(&router, &members.john, &refused).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{problem}");
    assert_eq!(
        harness::error_code(&problem),
        Some(error_code::INVITE_EMAIL_UNAVAILABLE.as_str())
    );
    assert_eq!(
        listed_status(&router, &members, &refused).await,
        EMAIL_STATUS_FAILED
    );

    let (revoked, _address) = invite(&router, &members).await;
    let revoked = id_of(&revoked).to_owned();
    let counted = members.email_attempts(&revoked).await;
    let revoke = format!("{INVITES}/{revoked}");
    let (status, _) = call(&router, Method::DELETE, &revoke, &members.john, "").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, problem) = send_again(&router, &members.john, &revoked).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    assert_eq!(
        harness::error_code(&problem),
        Some(error_code::INVITE_NOT_FOUND.as_str())
    );
    assert_eq!(
        members.email_attempts(&revoked).await,
        counted,
        "nothing counted"
    );
    assert!(relay.received().is_empty());
    members.cleanup().await;
}

/// The count of a send fails after the invite commits: the invite is still
/// created and listed, nothing is sent and nothing counted, and sending again
/// delivers it as the first attempt.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_email_count_failure_keeps_invite() {
    let members = Members::create().await;
    let relay = FakeRelay::start(vec![Session::Accept]).await;
    let (fleet, failpoint) = owner_fleet(&members, Some(relay.port()))
        .await
        .with_team_fault(TeamStep::BeginEmail, 1);
    let router = fleet.router();
    let (created, _address) = invite(&router, &members).await;
    let id = id_of(&created).to_owned();
    assert_eq!(failpoint.fired(), 1);
    assert!(relay.received().is_empty(), "nothing sent");
    assert_eq!(members.email_attempts(&id).await, 0, "nothing counted");
    assert_eq!(
        listed_status(&router, &members, &id).await,
        EMAIL_STATUS_FAILED
    );

    let (status, answered) = send_again(&router, &members.john, &id).await;
    assert_eq!(status, StatusCode::OK, "{answered}");
    let received = relay.received();
    assert_eq!(received.len(), 1);
    assert!(
        received
            .iter()
            .all(|message| message.contains(&key_header(&id, 1)))
    );
    assert_eq!(
        listed_status(&router, &members, &id).await,
        EMAIL_STATUS_SENT
    );
    members.cleanup().await;
}

/// The relay accepts but the record of it fails: the invite stands and the
/// create answers `sent`, while the row, never stamped, lists as `failed`.
/// Sending again then delivers a second message under the next key — the
/// cost of reading an unrecorded send as failed, pinned here so a change to
/// that choice is a visible one.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_email_unrecorded_keeps_invite() {
    let members = Members::create().await;
    let relay = FakeRelay::start(vec![Session::Accept, Session::Accept]).await;
    let (fleet, failpoint) = owner_fleet(&members, Some(relay.port()))
        .await
        .with_team_fault(TeamStep::RecordEmail, 1);
    let router = fleet.router();
    let (created, _address) = invite(&router, &members).await;
    let id = id_of(&created).to_owned();
    assert_eq!(failpoint.fired(), 1);
    assert_eq!(text(&created, "email_status"), Some(EMAIL_STATUS_SENT));
    assert_eq!(relay.received().len(), 1, "the relay took it");
    assert_eq!(members.email_attempts(&id).await, 1);
    assert_eq!(
        listed_status(&router, &members, &id).await,
        EMAIL_STATUS_FAILED
    );

    let (status, _) = send_again(&router, &members.john, &id).await;
    assert_eq!(status, StatusCode::OK);
    let received = relay.received();
    assert_eq!(
        received.len(),
        2,
        "a second message, not a deduplicated one"
    );
    assert!(
        received
            .last()
            .is_some_and(|message| message.contains(&key_header(&id, 2)))
    );
    assert_eq!(
        listed_status(&router, &members, &id).await,
        EMAIL_STATUS_SENT
    );
    members.cleanup().await;
}

/// The store refuses the invite itself: the route answers the outage, nothing
/// is listed, and no email goes.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_invite_store_failure_issues_nothing() {
    let members = Members::create().await;
    let relay = FakeRelay::start(vec![Session::Accept]).await;
    let (fleet, failpoint) = owner_fleet(&members, Some(relay.port()))
        .await
        .with_team_fault(TeamStep::Invite, 1);
    let router = fleet.router();
    let body = json!({ "email": format!("invitee+{}@example.test", mint_id()) }).to_string();
    let (status, problem) = call(&router, Method::POST, INVITES, &members.john, &body).await;
    assert_eq!(failpoint.fired(), 1);
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{problem}");
    assert_eq!(
        harness::error_code(&problem),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
    let (_, listed) = call(&router, Method::GET, INVITES, &members.john, "").await;
    assert!(items(&listed).is_empty(), "{listed}");
    assert!(relay.received().is_empty());
    members.cleanup().await;
}

/// The invite stops being sendable between its commit and its count — a
/// revoke landing in that gap: the create still answers 201 and nothing is
/// sent or counted.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_invite_gone_before_its_email_sends_nothing() {
    let members = Members::create().await;
    let relay = FakeRelay::start(vec![Session::Accept]).await;
    let (fleet, failpoint) = owner_fleet(&members, Some(relay.port()))
        .await
        .with_team_fault(TeamStep::BeginEmailGone, 1);
    let (created, _address) = invite(&fleet.router(), &members).await;
    assert_eq!(failpoint.fired(), 1);
    assert!(relay.received().is_empty());
    assert_eq!(members.email_attempts(id_of(&created)).await, 0);
    members.cleanup().await;
}

/// The store refuses a removal: the route answers the outage and the member
/// stays.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_member_removal_store_failure_keeps_member() {
    let members = Members::create().await;
    let relay = FakeRelay::start(vec![]).await;
    let (fleet, failpoint) = owner_fleet(&members, Some(relay.port()))
        .await
        .with_team_fault(TeamStep::Remove, 1);
    let router = fleet.router();
    let path = format!("{MEMBERS}/{}", members.bob.user);
    let (status, problem) = call(&router, Method::DELETE, &path, &members.john, "").await;
    assert_eq!(failpoint.fired(), 1);
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{problem}");
    assert_eq!(
        harness::error_code(&problem),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
    let (_, listed) = call(&router, Method::GET, MEMBERS, &members.john, "").await;
    let kept = items(&listed)
        .iter()
        .any(|item| text(item, "user_id") == Some(members.bob.user.as_str()));
    assert!(kept, "{listed}");
    members.cleanup().await;
}
