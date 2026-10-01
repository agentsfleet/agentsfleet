//! The invite email through the routes, over live Postgres and a real SMTP
//! exchange: Mailpit for delivery, a scripted relay for refusals, drops and
//! stalls. Every case also proves the invite outlives whatever email did.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_mail::{IDEMPOTENCY_HEADER, SMTP_RELAY_BAG};
use afd_vault::{SecretBody, SecretName};
use axum::Router;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::harness::{Fleet, send, vault};
use crate::integration_workspace_members::fixture::{Members, owner_scopes};

#[path = "support/fake_smtp.rs"]
mod fake_smtp;

use self::fake_smtp::{FakeRelay, Session};

const INVITES: &str = "/v1/tenants/me/invites";
const FROM: &str = "hello@agentsfleet.test";
const LOOPBACK: &str = "127.0.0.1";
const STALL_DEADLINE: Duration = Duration::from_millis(500);
const STATUS_SENT: &str = "sent";
const STATUS_FAILED: &str = "failed";
const MAILPIT_SMTP_PORT: &str = "TEST_MAILPIT_SMTP_PORT";
const MAILPIT_URL: &str = "TEST_MAILPIT_URL";

/// John's routes, with John's workspace as the platform admin when `relay`
/// names a port: the `smtp-relay` bag is sealed there, pointing at it.
async fn owner(members: &Members, relay: Option<u16>, deadline: Option<Duration>) -> Router {
    let mut fleet = Fleet::live(
        members.database.clone(),
        &members.john.subject,
        owner_scopes(),
    )
    .with_live_ownership()
    .with_dashboard_holding(&members.john.subject, owner_scopes());
    if let Some(port) = relay {
        seal_relay(members, port).await;
        fleet = fleet.with_platform_admin(members.john.workspace.clone());
    }
    if let Some(deadline) = deadline {
        fleet = fleet.with_mail_deadline(deadline);
    }
    fleet.router()
}

async fn seal_relay(members: &Members, port: u16) {
    let bag = json!({
        "host": LOOPBACK,
        "port": port.to_string(),
        "username": "relay",
        "password": "relay-password",
        "from_address": FROM,
    })
    .to_string();
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

async fn call(
    router: &Router,
    members: &Members,
    method: Method,
    path: &str,
    body: &str,
) -> (StatusCode, Value) {
    let response = send(router, method, path, Some(&members.john.token), body).await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a test body is in memory");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// John invites a fresh address; the 201 and the address it went to.
async fn invite(router: &Router, members: &Members) -> (Value, String) {
    let address = format!("invitee+{}@example.test", mint_id());
    let body = json!({ "email": address }).to_string();
    let (status, created) = call(router, members, Method::POST, INVITES, &body).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    (created, address)
}

fn text<'v>(value: &'v Value, key: &str) -> &'v str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The invite still pending in John's list, with the status it carries.
async fn listed_status(router: &Router, members: &Members, invite: &str) -> String {
    let (status, page) = call(router, members, Method::GET, INVITES, "").await;
    assert_eq!(status, StatusCode::OK);
    let items = page
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let item = items
        .iter()
        .find(|item| text(item, "id") == invite)
        .expect("the invite stays pending");
    text(item, "email_status").to_owned()
}

fn key_header(invite: &str, attempt: u32) -> String {
    format!("{IDEMPOTENCY_HEADER}: invite-{invite}-{attempt}")
}

async fn mailpit_get(path: &str) -> Value {
    let base =
        std::env::var(MAILPIT_URL).expect("make test-integration-rustd exports the Mailpit URL");
    let body = reqwest::get(format!("{base}{path}"))
        .await
        .expect("Mailpit answers")
        .text()
        .await
        .expect("Mailpit's answer reads");
    serde_json::from_str(&body).expect("Mailpit answers JSON")
}

/// Dimension 2.1: one message reaches Mailpit from the bag's address, naming
/// John and his account and carrying the accept link; the invite reads `sent`.
#[tokio::test]
#[ignore = "needs live Postgres and Mailpit: make test-integration-rustd"]
async fn test_invite_email_carries_accept_link() {
    let members = Members::create().await;
    members.seed().await;
    let port = std::env::var(MAILPIT_SMTP_PORT)
        .expect("make test-integration-rustd exports the Mailpit SMTP port")
        .parse()
        .expect("a port number");
    let router = owner(&members, Some(port), None).await;
    let (created, address) = invite(&router, &members).await;
    assert_eq!(text(&created, "email_status"), STATUS_SENT);
    assert!(
        created
            .get("email_sent_at")
            .and_then(Value::as_i64)
            .is_some()
    );

    let found = mailpit_get(&format!("/api/v1/search?query=to:{address}")).await;
    let messages = found
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert_eq!(messages.len(), 1, "{found}");
    let id = messages
        .first()
        .map(|message| text(message, "ID").to_owned())
        .unwrap_or_default();
    let message = mailpit_get(&format!("/api/v1/message/{id}")).await;
    let link = text(&created, "link");
    // pin test: literal is the contract
    assert_eq!(
        text(&message, "Subject"),
        "You're invited to join John's account on agentsfleet"
    );
    assert_eq!(
        message.pointer("/From/Address").and_then(Value::as_str),
        Some(FROM)
    );
    assert!(text(&message, "Text").contains(link) && text(&message, "Text").contains("John"));
    assert!(text(&message, "HTML").contains(link));
    let headers = mailpit_get(&format!("/api/v1/message/{id}/headers")).await;
    let key = headers
        .pointer(&format!("/{IDEMPOTENCY_HEADER}/0"))
        .and_then(Value::as_str);
    let expected = format!("invite-{}-1", text(&created, "id"));
    assert_eq!(key, Some(expected.as_str()));
    members.cleanup().await;
}

/// Dimension 2.2: a connection dropped after the message is retried once,
/// carrying the same idempotency key.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_send_retry_reuses_idempotency_key() {
    let members = Members::create().await;
    members.seed().await;
    let relay = FakeRelay::start(vec![Session::DropAfterData, Session::Accept]).await;
    let router = owner(&members, Some(relay.port), None).await;
    let (created, _address) = invite(&router, &members).await;
    assert_eq!(text(&created, "email_status"), STATUS_SENT);
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
    members.seed().await;
    let router = owner(&members, None, None).await;
    let (created, _address) = invite(&router, &members).await;
    let id = text(&created, "id");
    assert_eq!(text(&created, "email_status"), "unconfigured");
    assert_eq!(listed_status(&router, &members, id).await, "unconfigured");
    members.cleanup().await;
}

/// Dimension 3.2: a refused login, a refused recipient either way, and a
/// relay that never answers each leave a pending invite reading `failed`.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_failed_email_keeps_invite() {
    let members = Members::create().await;
    members.seed().await;
    let script = vec![
        Session::RefuseAuth(535),
        Session::RefuseRecipient(450),
        Session::RefuseRecipient(550),
        Session::Stall,
        Session::Stall,
    ];
    let relay = FakeRelay::start(script).await;
    let router = owner(&members, Some(relay.port), Some(STALL_DEADLINE)).await;
    for _case in 0..4 {
        let (created, _address) = invite(&router, &members).await;
        assert_eq!(text(&created, "email_status"), STATUS_FAILED);
        assert_eq!(
            listed_status(&router, &members, text(&created, "id")).await,
            STATUS_FAILED
        );
    }
    assert!(relay.received().is_empty());
    members.cleanup().await;
}

/// Dimension 4.1: sending again after a failure is a new attempt under a new
/// key, and the invite flips to `sent`.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_send_again_after_failure() {
    let members = Members::create().await;
    members.seed().await;
    let relay = FakeRelay::start(vec![Session::RefuseAuth(535), Session::Accept]).await;
    let router = owner(&members, Some(relay.port), None).await;
    let (created, _address) = invite(&router, &members).await;
    let id = text(&created, "id").to_owned();
    assert_eq!(text(&created, "email_status"), STATUS_FAILED);

    let path = format!("{INVITES}/{id}/send");
    let (status, answered) = call(&router, &members, Method::POST, &path, "").await;
    assert_eq!(status, StatusCode::OK, "{answered}");
    assert_eq!(text(&answered, "email_status"), STATUS_SENT);
    let received = relay.received();
    assert_eq!(received.len(), 1);
    assert!(
        received
            .iter()
            .all(|message| message.contains(&key_header(&id, 2)))
    );
    assert_eq!(listed_status(&router, &members, &id).await, STATUS_SENT);
    members.cleanup().await;
}

/// Dimension 4.2: sending again with no relay is `503 UZ-INV-005`, and an
/// invite that is not pending is `404 UZ-INV-001`.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_send_again_unconfigured_refused() {
    let members = Members::create().await;
    members.seed().await;
    let router = owner(&members, None, None).await;
    let (created, _address) = invite(&router, &members).await;
    let path = format!("{INVITES}/{}/send", text(&created, "id"));
    let (status, problem) = call(&router, &members, Method::POST, &path, "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        text(&problem, "error_code"),
        error_code::INVITE_EMAIL_UNAVAILABLE.as_str()
    );

    let unknown = Uuid7::parse(&mint_id()).expect("a minted id is canonical");
    let (status, problem) = call(
        &router,
        &members,
        Method::POST,
        &format!("{INVITES}/{unknown}/send"),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        text(&problem, "error_code"),
        error_code::INVITE_NOT_FOUND.as_str()
    );
    members.cleanup().await;
}
