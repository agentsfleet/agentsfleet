//! One invite email delivered for real: through Mailpit's SMTP listener, read
//! back over its API, carrying the accept link and the idempotency key.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_mail::IDEMPOTENCY_HEADER;
use afd_mail::test_util::FROM;
use afd_tenant::team::EMAIL_STATUS_SENT;
use serde_json::Value;

use crate::harness::text;
use crate::integration_invite_email::{id_of, invite, owner_fleet};
use crate::integration_workspace_members::fixture::Members;

const MAILPIT_SMTP_PORT: &str = "TEST_MAILPIT_SMTP_PORT";
const MAILPIT_URL: &str = "TEST_MAILPIT_URL";

async fn mailpit_get(path: &str) -> Value {
    mailpit_fetch(mailpit_url(path, &[])).await
}

/// The messages addressed to `address`. The query rides percent-encoded: a
/// raw `+` in an address would decode as a space and match every earlier
/// run's mail, since Mailpit is not reset between runs.
async fn mailpit_search_to(address: &str) -> Value {
    let query = format!("to:{address}");
    mailpit_fetch(mailpit_url("/api/v1/search", &[("query", query.as_str())])).await
}

fn mailpit_url(path: &str, query: &[(&str, &str)]) -> reqwest::Url {
    let base =
        std::env::var(MAILPIT_URL).expect("make test-integration-rustd exports the Mailpit URL");
    reqwest::Url::parse_with_params(&format!("{base}{path}"), query).expect("a Mailpit URL")
}

async fn mailpit_fetch(url: reqwest::Url) -> Value {
    let body = reqwest::get(url)
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
    let port = std::env::var(MAILPIT_SMTP_PORT)
        .expect("make test-integration-rustd exports the Mailpit SMTP port")
        .parse()
        .expect("a port number");
    let router = owner_fleet(&members, Some(port)).await.router();
    let (created, address) = invite(&router, &members).await;
    assert_eq!(text(&created, "email_status"), Some(EMAIL_STATUS_SENT));
    assert!(
        created
            .get("email_sent_at")
            .and_then(Value::as_i64)
            .is_some()
    );

    let found = mailpit_search_to(&address).await;
    let messages = found
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert_eq!(messages.len(), 1, "{found}");
    let id = messages
        .first()
        .and_then(|message| text(message, "ID"))
        .expect("Mailpit names the message");
    let message = mailpit_get(&format!("/api/v1/message/{id}")).await;
    let link = text(&created, "link").expect("the invite answers its link");
    // pin test: literal is the contract
    assert_eq!(
        text(&message, "Subject"),
        Some("You're invited to join John's account on agentsfleet")
    );
    assert_eq!(
        message.pointer("/From/Address").and_then(Value::as_str),
        Some(FROM)
    );
    let plain = text(&message, "Text").expect("the message has a text part");
    assert!(plain.contains(link) && plain.contains("John"));
    let html = text(&message, "HTML").expect("the message has an HTML part");
    assert!(html.contains(link));
    let headers = mailpit_get(&format!("/api/v1/message/{id}/headers")).await;
    let key = headers
        .pointer(&format!("/{IDEMPOTENCY_HEADER}/0"))
        .and_then(Value::as_str);
    let expected = format!("invite-{}-1", id_of(&created));
    assert_eq!(key, Some(expected.as_str()));
    members.cleanup().await;
}
