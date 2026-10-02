//! The invite email through the routes, over live Postgres and a real SMTP
//! exchange: Mailpit for delivery, a scripted relay for refusals, drops and
//! stalls. Every case also proves the invite outlives whatever email did.
//!
//! Delivery to Mailpit is `integration_invite_email_mailpit.rs`; sending
//! again, and the team store failing under a send, are
//! `integration_invite_email_send.rs`. Both share the helpers here.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_db::test_util::mint_id;
use afd_mail::test_util::{FakeRelay, LOOPBACK, Session, bag_json};
use afd_mail::{IDEMPOTENCY_HEADER, SMTP_RELAY_BAG};
use afd_observability::{InviteEmailOutcome, Recorded, Telemetry};
use afd_tenant::team::{EMAIL_STATUS_FAILED, EMAIL_STATUS_SENT, EMAIL_STATUS_UNCONFIGURED};
use afd_vault::{SecretBody, SecretName};
use axum::Router;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::harness::{Fleet, items, send, vault};
use crate::integration_team_routes::{INVITES, call};
use crate::integration_workspace_members::fixture::{Members, owner_scopes};

const STALL_DEADLINE: Duration = Duration::from_millis(500);

/// John's daemon, with John's workspace as the platform admin when `relay`
/// names a port: the `smtp-relay` bag is sealed there, pointing at it.
///
/// A builder, so a case chains the one seam it is about — a send deadline, a
/// broken team-store call, recorded analytics — before it routes.
pub(crate) async fn owner_fleet(members: &Members, relay: Option<u16>) -> Fleet {
    let fleet = members.fleet(&members.john, owner_scopes());
    match relay {
        Some(port) => {
            seal_relay(members, port).await;
            fleet.with_platform_admin(members.john.workspace.clone())
        }
        None => fleet,
    }
}

async fn seal_relay(members: &Members, port: u16) {
    let bag = bag_json(LOOPBACK, port);
    let raw = serde_json::value::RawValue::from_string(bag).expect("the bag is an object");
    let sealed = vault(members.database.clone())
        .create(
            &members.john.workspace,
            &SecretName::parse(SMTP_RELAY_BAG).expect("a storable name"),
            &SecretBody::parse(&raw).expect("a storable body"),
            UnixMillis::from_millis(1),
        )
        .await;
    assert!(sealed.is_ok(), "the smtp-relay bag seals: {sealed:?}");
}

/// John invites a fresh address; the 201 and the address it went to.
pub(crate) async fn invite(router: &Router, members: &Members) -> (Value, String) {
    let address = format!("invitee+{}@example.test", mint_id());
    let body = json!({ "email": address }).to_string();
    let (status, created) = call(router, Method::POST, INVITES, &members.john, &body).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    (created, address)
}

/// A string field, or empty when the value carries none.
pub(crate) fn text<'v>(value: &'v Value, key: &str) -> &'v str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The invite still pending in John's list, with the status it carries.
pub(crate) async fn listed_status(router: &Router, members: &Members, invite: &str) -> String {
    let (status, page) = call(router, Method::GET, INVITES, &members.john, "").await;
    assert_eq!(status, StatusCode::OK);
    let item = items(&page)
        .iter()
        .find(|item| text(item, "id") == invite)
        .expect("the invite stays pending");
    text(item, "email_status").to_owned()
}

/// The idempotency header a send of `invite` carries on its `attempt`.
pub(crate) fn key_header(invite: &str, attempt: u32) -> String {
    format!("{IDEMPOTENCY_HEADER}: invite-{invite}-{attempt}")
}

/// The invite-email events reported for `invite`: attempt and outcome.
fn reported(recorded: &Recorded, invite: &str) -> Vec<(i32, InviteEmailOutcome)> {
    recorded
        .events()
        .into_iter()
        .filter_map(|event| match event {
            Telemetry::InviteEmail {
                invite_id,
                attempt,
                outcome,
                ..
            } if invite_id == invite => Some((attempt, outcome)),
            _ => None,
        })
        .collect()
}

/// Dimension 2.2: a connection dropped after the message is retried once,
/// carrying the same idempotency key.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_send_retry_reuses_idempotency_key() {
    let members = Members::create().await;
    let relay = FakeRelay::start(vec![Session::DropAfterData, Session::Accept]).await;
    let router = owner_fleet(&members, Some(relay.port())).await.router();
    let (created, _address) = invite(&router, &members).await;
    assert_eq!(text(&created, "email_status"), EMAIL_STATUS_SENT);
    let received = relay.received();
    assert_eq!(received.len(), 2);
    let key = key_header(text(&created, "id"), 1);
    assert!(
        received.iter().all(|message| message.contains(&key)),
        "{received:?}"
    );
    members.cleanup().await;
}

/// Dimension 3.1: with no relay the invite still issues, reads
/// `unconfigured`, and stays pending.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_unconfigured_email_keeps_invite() {
    let members = Members::create().await;
    let router = owner_fleet(&members, None).await.router();
    let (created, _address) = invite(&router, &members).await;
    let id = text(&created, "id");
    assert_eq!(text(&created, "email_status"), EMAIL_STATUS_UNCONFIGURED);
    assert_eq!(
        listed_status(&router, &members, id).await,
        EMAIL_STATUS_UNCONFIGURED
    );
    members.cleanup().await;
}

/// Dimension 3.2: a refused login, a refused recipient either way, and a
/// relay that never answers each leave a pending invite reading `failed`.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_failed_email_keeps_invite() {
    let members = Members::create().await;
    let script = vec![
        Session::RefuseAuth(535),
        Session::RefuseRecipient(450),
        Session::RefuseRecipient(550),
        Session::Stall,
        Session::Stall,
    ];
    let relay = FakeRelay::start(script).await;
    let router = owner_fleet(&members, Some(relay.port()))
        .await
        .with_mail_deadline(STALL_DEADLINE)
        .router();
    for _case in 0..4 {
        let (created, _address) = invite(&router, &members).await;
        assert_eq!(text(&created, "email_status"), EMAIL_STATUS_FAILED);
        assert_eq!(
            listed_status(&router, &members, text(&created, "id")).await,
            EMAIL_STATUS_FAILED
        );
    }
    assert!(relay.received().is_empty());
    members.cleanup().await;
}

/// An invite whose email failed is still a working invite: its invitee sees
/// it waiting and accepts it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_failed_email_invite_is_still_acceptable() {
    let members = Members::create().await;
    let relay = FakeRelay::start(vec![Session::RefuseRecipient(550)]).await;
    let router = owner_fleet(&members, Some(relay.port())).await.router();
    let body = json!({ "email": members.stranger.email }).to_string();
    let (status, created) = call(&router, Method::POST, INVITES, &members.john, &body).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(text(&created, "email_status"), EMAIL_STATUS_FAILED);
    let id = text(&created, "id");

    let strangers = members.router(&members.stranger, owner_scopes());
    let accept = format!("/v1/users/me/invites/{id}/accept");
    let response = send(
        &strangers,
        Method::POST,
        &accept,
        Some(&members.stranger.token),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    members.cleanup().await;
}

/// Each send reports one product event naming its attempt and what became of
/// it — sent with the relay's code, refused with its code, or unconfigured —
/// on behalf of the owner who sent it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_invite_email_reports_each_outcome() {
    let members = Members::create().await;
    let relay = FakeRelay::start(vec![Session::Accept, Session::RefuseRecipient(550)]).await;
    let (fleet, recorded) = owner_fleet(&members, Some(relay.port()))
        .await
        .with_recorded_analytics();
    let router = fleet.router();
    let (sent, _address) = invite(&router, &members).await;
    let (refused, _address) = invite(&router, &members).await;
    assert_eq!(
        reported(&recorded, text(&sent, "id")),
        [(1, InviteEmailOutcome::Sent { reply: 250 })]
    );
    assert_eq!(
        reported(&recorded, text(&refused, "id")),
        [(1, InviteEmailOutcome::Failed { reply: Some(550) })]
    );
    let actors: Vec<String> = recorded
        .events()
        .into_iter()
        .filter_map(|event| match event {
            Telemetry::InviteEmail {
                actor, tenant_id, ..
            } => {
                assert_eq!(tenant_id, members.john.tenant);
                Some(actor)
            }
            _ => None,
        })
        .collect();
    assert_eq!(actors, [members.john.subject.as_str(); 2]);

    let (unconfigured, quietly) = owner_fleet(&members, None).await.with_recorded_analytics();
    let (quiet, _address) = invite(&unconfigured.router(), &members).await;
    assert_eq!(
        reported(&quietly, text(&quiet, "id")),
        [(1, InviteEmailOutcome::Unconfigured)]
    );
    members.cleanup().await;
}
