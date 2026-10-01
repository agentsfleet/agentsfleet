//! The invite email through the routes, over live Postgres and a real SMTP
//! exchange: Mailpit for delivery, a scripted relay for refusals, drops and
//! stalls. Every case also proves the invite outlives whatever email did.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_mail::test_util::{FROM, FakeRelay, LOOPBACK, Session, bag_json};
use afd_mail::{IDEMPOTENCY_HEADER, SMTP_RELAY_BAG};
use afd_observability::{InviteEmailOutcome, Recorded, Telemetry};
use afd_tenant::team::{EMAIL_STATUS_FAILED, EMAIL_STATUS_SENT, EMAIL_STATUS_UNCONFIGURED};
use afd_vault::{SecretBody, SecretName};
use axum::Router;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::harness::{Failpoint, Fleet, TeamStep, send, vault};
use crate::integration_workspace_members::fixture::{Members, owner_scopes};

const INVITES: &str = "/v1/tenants/me/invites";
const MEMBERS: &str = "/v1/tenants/me/members";
const STALL_DEADLINE: Duration = Duration::from_millis(500);
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

/// John's routes over a live relay at `port`, with one team-store write
/// broken on its first call.
async fn owner_breaking(members: &Members, port: u16, step: TeamStep) -> (Router, Arc<Failpoint>) {
    seal_relay(members, port).await;
    let (fleet, failpoint) = Fleet::live(
        members.database.clone(),
        &members.john.subject,
        owner_scopes(),
    )
    .with_live_ownership()
    .with_dashboard_holding(&members.john.subject, owner_scopes())
    .with_platform_admin(members.john.workspace.clone())
    .with_team_fault(step, 1);
    (fleet.router(), failpoint)
}

/// John's routes, keeping every product event they report.
async fn owner_recording(members: &Members, relay: Option<u16>) -> (Router, Recorded) {
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
    let (fleet, recorded) = fleet.with_recorded_analytics();
    (fleet.router(), recorded)
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
    members.seed().await;
    let port = std::env::var(MAILPIT_SMTP_PORT)
        .expect("make test-integration-rustd exports the Mailpit SMTP port")
        .parse()
        .expect("a port number");
    let router = owner(&members, Some(port), None).await;
    let (created, address) = invite(&router, &members).await;
    assert_eq!(text(&created, "email_status"), EMAIL_STATUS_SENT);
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
    let router = owner(&members, Some(relay.port()), None).await;
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
    members.seed().await;
    let router = owner(&members, None, None).await;
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
    members.seed().await;
    let script = vec![
        Session::RefuseAuth(535),
        Session::RefuseRecipient(450),
        Session::RefuseRecipient(550),
        Session::Stall,
        Session::Stall,
    ];
    let relay = FakeRelay::start(script).await;
    let router = owner(&members, Some(relay.port()), Some(STALL_DEADLINE)).await;
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

/// Dimension 4.1: sending again after a failure is a new attempt under a new
/// key, and the invite flips to `sent`.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_send_again_after_failure() {
    let members = Members::create().await;
    members.seed().await;
    let relay = FakeRelay::start(vec![Session::RefuseAuth(535), Session::Accept]).await;
    let router = owner(&members, Some(relay.port()), None).await;
    let (created, _address) = invite(&router, &members).await;
    let id = text(&created, "id").to_owned();
    assert_eq!(text(&created, "email_status"), EMAIL_STATUS_FAILED);

    let path = format!("{INVITES}/{id}/send");
    let (status, answered) = call(&router, &members, Method::POST, &path, "").await;
    assert_eq!(status, StatusCode::OK, "{answered}");
    assert_eq!(text(&answered, "email_status"), EMAIL_STATUS_SENT);
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

/// How many sends an invite has counted, read from the row.
async fn attempts_of(members: &Members, invite: &str) -> i32 {
    let mut connection = members.database.acquire().await.expect("a connection");
    sqlx::query_scalar("SELECT email_attempts FROM core.invites WHERE id = $1::uuid")
        .bind(invite)
        .fetch_one(&mut *connection)
        .await
        .expect("the invite row reads")
}

/// The count of a send fails after the invite commits: the invite is still
/// created and listed, nothing is sent and nothing counted, and sending again
/// delivers it as the first attempt.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_email_count_failure_keeps_invite() {
    let members = Members::create().await;
    members.seed().await;
    let relay = FakeRelay::start(vec![Session::Accept]).await;
    let (router, failpoint) = owner_breaking(&members, relay.port(), TeamStep::BeginEmail).await;
    let (created, _address) = invite(&router, &members).await;
    let id = text(&created, "id").to_owned();
    assert_eq!(failpoint.fired(), 1);
    assert!(relay.received().is_empty(), "nothing sent");
    assert_eq!(attempts_of(&members, &id).await, 0, "nothing counted");
    assert_eq!(
        listed_status(&router, &members, &id).await,
        EMAIL_STATUS_FAILED
    );

    let path = format!("{INVITES}/{id}/send");
    let (status, answered) = call(&router, &members, Method::POST, &path, "").await;
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
    members.seed().await;
    let relay = FakeRelay::start(vec![Session::Accept, Session::Accept]).await;
    let (router, failpoint) = owner_breaking(&members, relay.port(), TeamStep::RecordEmail).await;
    let (created, _address) = invite(&router, &members).await;
    let id = text(&created, "id").to_owned();
    assert_eq!(failpoint.fired(), 1);
    assert_eq!(text(&created, "email_status"), EMAIL_STATUS_SENT);
    assert_eq!(relay.received().len(), 1, "the relay took it");
    assert_eq!(attempts_of(&members, &id).await, 1);
    assert_eq!(
        listed_status(&router, &members, &id).await,
        EMAIL_STATUS_FAILED
    );

    let path = format!("{INVITES}/{id}/send");
    let (status, _) = call(&router, &members, Method::POST, &path, "").await;
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

/// Sending again: a revoked invite is not found and counts nothing; a relay
/// that refuses is `503 UZ-INV-005` and the invite lists as `failed`.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_send_again_refusals_leave_the_row_true() {
    let members = Members::create().await;
    members.seed().await;
    let relay = FakeRelay::start(vec![
        Session::RefuseRecipient(550),
        Session::RefuseRecipient(550),
        Session::RefuseRecipient(550),
    ])
    .await;
    let router = owner(&members, Some(relay.port()), None).await;

    let (refused, _address) = invite(&router, &members).await;
    let refused = text(&refused, "id").to_owned();
    let path = format!("{INVITES}/{refused}/send");
    let (status, problem) = call(&router, &members, Method::POST, &path, "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{problem}");
    assert_eq!(
        text(&problem, "error_code"),
        error_code::INVITE_EMAIL_UNAVAILABLE.as_str()
    );
    assert_eq!(
        listed_status(&router, &members, &refused).await,
        EMAIL_STATUS_FAILED
    );

    let (revoked, _address) = invite(&router, &members).await;
    let revoked = text(&revoked, "id").to_owned();
    let counted = attempts_of(&members, &revoked).await;
    let (status, _) = call(
        &router,
        &members,
        Method::DELETE,
        &format!("{INVITES}/{revoked}"),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, problem) = call(
        &router,
        &members,
        Method::POST,
        &format!("{INVITES}/{revoked}/send"),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    assert_eq!(
        text(&problem, "error_code"),
        error_code::INVITE_NOT_FOUND.as_str()
    );
    assert_eq!(
        attempts_of(&members, &revoked).await,
        counted,
        "nothing counted"
    );
    assert!(relay.received().is_empty());
    members.cleanup().await;
}

/// An invite whose email failed is still a working invite: its invitee sees
/// it waiting and accepts it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_failed_email_invite_is_still_acceptable() {
    let members = Members::create().await;
    members.seed().await;
    let relay = FakeRelay::start(vec![Session::RefuseRecipient(550)]).await;
    let router = owner(&members, Some(relay.port()), None).await;
    let body = json!({ "email": members.stranger.email }).to_string();
    let (status, created) = call(&router, &members, Method::POST, INVITES, &body).await;
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
    members.seed().await;
    let relay = FakeRelay::start(vec![Session::Accept, Session::RefuseRecipient(550)]).await;
    let (router, recorded) = owner_recording(&members, Some(relay.port())).await;
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
        .iter()
        .filter_map(|event| match event {
            Telemetry::InviteEmail {
                actor, tenant_id, ..
            } => {
                assert_eq!(tenant_id, &members.john.tenant);
                Some(actor.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        actors,
        [members.john.subject.clone(), members.john.subject.clone()]
    );

    let (unconfigured_router, unconfigured) = owner_recording(&members, None).await;
    let (quiet, _address) = invite(&unconfigured_router, &members).await;
    assert_eq!(
        reported(&unconfigured, text(&quiet, "id")),
        [(1, InviteEmailOutcome::Unconfigured)]
    );
    members.cleanup().await;
}

/// The store refuses the invite itself: the route answers the outage, nothing
/// is listed, and no email goes.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_invite_store_failure_issues_nothing() {
    let members = Members::create().await;
    members.seed().await;
    let relay = FakeRelay::start(vec![Session::Accept]).await;
    let (router, failpoint) = owner_breaking(&members, relay.port(), TeamStep::Invite).await;
    let body = json!({ "email": format!("invitee+{}@example.test", mint_id()) }).to_string();
    let (status, problem) = call(&router, &members, Method::POST, INVITES, &body).await;
    assert_eq!(failpoint.fired(), 1);
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{problem}");
    let (_, listed) = call(&router, &members, Method::GET, INVITES, "").await;
    let items = listed
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(items.is_empty(), "{listed}");
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
    members.seed().await;
    let relay = FakeRelay::start(vec![Session::Accept]).await;
    let (router, failpoint) =
        owner_breaking(&members, relay.port(), TeamStep::BeginEmailGone).await;
    let (created, _address) = invite(&router, &members).await;
    assert_eq!(failpoint.fired(), 1);
    assert!(relay.received().is_empty());
    assert_eq!(attempts_of(&members, text(&created, "id")).await, 0);
    members.cleanup().await;
}

/// The store refuses a removal: the route answers the outage and the member
/// stays.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_member_removal_store_failure_keeps_member() {
    let members = Members::create().await;
    members.seed().await;
    let relay = FakeRelay::start(vec![]).await;
    let (router, failpoint) = owner_breaking(&members, relay.port(), TeamStep::Remove).await;
    let path = format!("{MEMBERS}/{}", members.bob.user);
    let (status, problem) = call(&router, &members, Method::DELETE, &path, "").await;
    assert_eq!(failpoint.fired(), 1);
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{problem}");
    let (_, listed) = call(&router, &members, Method::GET, MEMBERS, "").await;
    let kept = listed
        .get("items")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| text(item, "user_id") == members.bob.user)
        });
    assert!(kept, "{listed}");
    members.cleanup().await;
}
