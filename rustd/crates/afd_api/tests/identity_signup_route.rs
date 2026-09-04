//! What the signup route proves before it opens an account.
//!
//! `POST /v1/auth/identity-events/clerk` is the only public route in this
//! daemon that CREATES a tenant, a user and a workspace, and the only proof its
//! caller offers is a signature over the body. Every case here is one that must
//! never reach the store — which is exactly what makes them provable with no
//! datastore: the fixture's pool is unreachable, so a refusal that leaked
//! through would fail as a connection error rather than passing quietly.
//!
//! The provisioning half — the five rows, the replay, the wallet heal — needs a
//! live Postgres and lives in `integration_identity_signup.rs`.
//!
//! # Why the unconfigured case is first
//!
//! It is the first thing the route decides, before the body is read as anything
//! but bytes. A deployment that configured no secret refuses every delivery,
//! because accepting an unverified one on the route that CREATES ACCOUNTS is
//! strictly worse than serving none.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use crate::harness;

use afd_core::error_code::{self, ErrorCode};
use afd_crypto::mac::HmacSha256Tag;
use afd_crypto::secret::SecretBytes;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use http::{HeaderName, Method, StatusCode};
use serde_json::Value;

use afd_webhook::vendor::svix;

use afd_auth::scope::ScopeSet;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_identity::MetadataUnwritten;

use self::harness::{Fleet, RecordingWriteback, json_body, send_with_headers};

/// Where a signup event arrives.
const PATH: &str = "/v1/auth/identity-events/clerk";

/// The secret this fixture deployment verifies against.
///
/// Carries the `whsec_` prefix and a base64 body because that is what the
/// vendor's own format is — a secret without it does not parse, and a test
/// using a bare string would be proving the parse rather than the wall.
const SECRET: &str = "whsec_C2FVsBQIhrscChlQIMV+b5sSYspob7oD";

/// The delivery id, which is the first field of the signed payload.
const DELIVERY: &str = "msg_2fJk8Lq0PsWzXbYtRnVdEcHgMa";

/// A `user.created` this daemon can open an account from.
const CREATED: &str = r#"{"type":"user.created","data":{"id":"user_2fJk8Lq0","email_addresses":[{"id":"idn_1","email_address":"ada@example.test"}],"primary_email_address_id":"idn_1","first_name":"Ada","last_name":"Lovelace"}}"#;

/// The instant this fixture signs and verifies at — frozen, not the wall clock.
fn now() -> i64 {
    harness::frozen_unix_seconds()
}

/// Signs exactly as the verifier expects, so a passing case is a round trip
/// rather than a restatement of the implementation's own output.
fn sign(id: &str, timestamp: i64, body: &str) -> String {
    let stamp = timestamp.to_string();
    let raw = STANDARD
        .decode(
            SECRET
                .strip_prefix("whsec_")
                .expect("the fixture carries the vendor's prefix"),
        )
        .expect("the fixture secret is base64");
    let tag = HmacSha256Tag::compute_peppered(
        &SecretBytes::new(raw),
        &[id.as_bytes(), b".", stamp.as_bytes(), b".", body.as_bytes()],
    );
    format!("v1,{}", STANDARD.encode(tag.as_bytes()))
}

/// The three headers a Svix delivery carries.
fn headers<'d>(id: &'d str, timestamp: &'d str, signature: &'d str) -> [(HeaderName, &'d str); 3] {
    [
        (HeaderName::from_static(svix::ID_HEADER), id),
        (HeaderName::from_static(svix::TIMESTAMP_HEADER), timestamp),
        (HeaderName::from_static(svix::SIGNATURE_HEADER), signature),
    ]
}

/// The registry code, as it is spelled on the wire.
fn code(code: ErrorCode) -> String {
    code.as_str().to_owned()
}

/// A correctly-signed delivery of `body`, against a configured deployment.
async fn signed(body: &str) -> http::Response<axum::body::Body> {
    let router = Fleet::new().with_identity_secret(SECRET).router();
    let signature = sign(DELIVERY, now(), body);
    send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        body,
        &headers(DELIVERY, &now().to_string(), &signature),
    )
    .await
}

/// The registry code a refusal carries.
async fn refusal_code(answer: http::Response<axum::body::Body>) -> String {
    json_body(answer)
        .await
        .get("error_code")
        .and_then(Value::as_str)
        .expect("every refusal carries its registry code")
        .to_owned()
}

#[tokio::test]
async fn a_deployment_with_no_configured_secret_refuses_every_delivery() {
    // Fail-closed, and the FIRST thing the route decides. The default fixture
    // leaves the secret unset, which is the real state of a deployment that
    // never configured one.
    let router = Fleet::new().router();
    let signature = sign(DELIVERY, now(), CREATED);
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        CREATED,
        &headers(DELIVERY, &now().to_string(), &signature),
    )
    .await;

    assert_eq!(
        refusal_code(answer).await,
        code(error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED),
        "an absent secret is unconfigured, never a failed verification — the \
         two are told apart by the code, which is what an operator reads"
    );
}

#[tokio::test]
async fn a_signature_under_the_wrong_key_is_refused_before_the_body_is_read() {
    let router = Fleet::new().with_identity_secret(SECRET).router();
    let forged = format!("v1,{}", STANDARD.encode([0x11_u8; 32]));
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        CREATED,
        &headers(DELIVERY, &now().to_string(), &forged),
    )
    .await;

    assert_eq!(
        refusal_code(answer).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID)
    );
}

#[tokio::test]
async fn a_tampered_body_no_longer_verifies() {
    // The signature is taken over the ORIGINAL body and presented with an
    // altered one — the case that proves the tag covers the payload and not
    // just the headers.
    let router = Fleet::new().with_identity_secret(SECRET).router();
    let signature = sign(DELIVERY, now(), CREATED);
    let tampered = CREATED.replace("ada@example.test", "mallory@example.test");
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        &tampered,
        &headers(DELIVERY, &now().to_string(), &signature),
    )
    .await;

    assert_eq!(
        refusal_code(answer).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID),
        "an address swapped after signing must not open an account"
    );
}

#[tokio::test]
async fn a_delivery_resent_under_a_fresh_id_no_longer_verifies() {
    // `svix-id` is the FIRST field of the signed payload, so it is not an
    // unauthenticated hint: a captured delivery replayed under a new id fails
    // the tag rather than opening a second account.
    let router = Fleet::new().with_identity_secret(SECRET).router();
    let signature = sign(DELIVERY, now(), CREATED);
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        CREATED,
        &headers(
            "msg_a_different_delivery_id",
            &now().to_string(),
            &signature,
        ),
    )
    .await;

    assert_eq!(
        refusal_code(answer).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID)
    );
}

#[tokio::test]
async fn a_delivery_outside_its_window_is_stale_rather_than_forged() {
    // Two refusals an operator must be able to tell apart: somebody replaying
    // an old capture, and somebody probing with a bad key.
    let router = Fleet::new().with_identity_secret(SECRET).router();
    let long_ago = now() - ONE_DAY_SECONDS;
    let signature = sign(DELIVERY, long_ago, CREATED);
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        CREATED,
        &headers(DELIVERY, &long_ago.to_string(), &signature),
    )
    .await;

    assert_eq!(
        refusal_code(answer).await,
        code(error_code::WEBHOOK_TIMESTAMP_STALE)
    );
}

/// A day in seconds — well past any freshness window this route enforces.
const ONE_DAY_SECONDS: i64 = 60 * 60 * 24;

#[tokio::test]
async fn a_verified_body_that_is_not_an_identity_event_is_refused() {
    let answer = signed(r#"{"not":"an event"}"#).await;
    assert_eq!(
        refusal_code(answer).await,
        code(error_code::INVALID_REQUEST),
        "a verified body this route cannot read is the sender's fault"
    );
}

#[tokio::test]
async fn an_event_this_daemon_serves_no_rule_for_is_answered_rather_than_refused() {
    // 200, never a 4xx. Every one of these is a real, correctly-signed
    // delivery; answering an error would put it in the provider's retry queue
    // forever, and retrying changes nothing about the event's type.
    let answer = signed(r#"{"type":"user.updated","data":{"id":"user_2fJk8Lq0"}}"#).await;
    assert_eq!(answer.status(), StatusCode::OK);
    assert_eq!(
        json_body(answer)
            .await
            .get("ignored")
            .and_then(Value::as_str),
        Some("user.updated")
    );
}

#[tokio::test]
async fn the_account_deletion_event_is_ignored_rather_than_acted_on() {
    // Deliberately NOT ported. Tearing an account down is a destructive path
    // with its own blast radius, and landing it under cover of the route that
    // OPENS accounts would ship a delete nobody reviewed. Pinned as a test so
    // the gap is a decision rather than an oversight.
    let answer = signed(r#"{"type":"user.deleted","data":{"id":"user_2fJk8Lq0"}}"#).await;
    assert_eq!(answer.status(), StatusCode::OK);
    assert_eq!(
        json_body(answer)
            .await
            .get("ignored")
            .and_then(Value::as_str),
        Some("user.deleted"),
        "an unported destructive path must answer as unhandled, never act"
    );
}

#[tokio::test]
async fn an_event_naming_no_primary_address_is_refused_before_the_store() {
    // The fixture's pool is unreachable, so a refusal that leaked through would
    // surface as a connection error rather than as this code.
    let answer = signed(
        r#"{"type":"user.created","data":{"id":"user_2fJk8Lq0","email_addresses":[{"id":"idn_1","email_address":"ada@example.test"}]}}"#,
    )
    .await;
    assert_eq!(
        refusal_code(answer).await,
        code(error_code::INVALID_REQUEST)
    );
}

#[tokio::test]
async fn an_address_the_provider_did_not_mark_primary_is_not_substituted() {
    // The one that matters most in this file. Falling back to the first address
    // in the list would open an account under whichever address happened to
    // sort first — somebody else's inbox, when a provider reports several.
    let answer = signed(
        r#"{"type":"user.created","data":{"id":"user_2fJk8Lq0","email_addresses":[{"id":"idn_1","email_address":"ada@example.test"}],"primary_email_address_id":"idn_absent"}}"#,
    )
    .await;
    assert_eq!(
        refusal_code(answer).await,
        code(error_code::INVALID_REQUEST),
        "a primary id naming no address must refuse, never fall back to the list"
    );
}

#[tokio::test]
async fn an_address_with_no_local_part_is_refused_rather_than_renamed() {
    // The Zig substitutes a fixed tenant name here. That hides a malformed
    // event behind a tenant nobody can tell from another; this refuses.
    let answer = signed(
        r#"{"type":"user.created","data":{"id":"user_2fJk8Lq0","email_addresses":[{"id":"idn_1","email_address":"@example.test"}],"primary_email_address_id":"idn_1"}}"#,
    )
    .await;
    assert_eq!(
        refusal_code(answer).await,
        code(error_code::INVALID_REQUEST)
    );
}

/// The whole point of the endpoint, over a real schema.
///
/// Every other case in this file is a refusal, and refusals are decided on
/// bytes the handler already holds — none of them reaches the store. That left
/// the one behaviour the route exists for ungraded: a verified `user.created`
/// opening a personal account, and the `Signups` adapter that carries it to
/// `afd_tenant` running only in production.
///
/// # The replay half is the load-bearing half
///
/// An identity provider retries, and the module says a retry must answer as the
/// first delivery did: 200 with `created: false`, naming the SAME workspace.
/// A 409 there would put a delivery the provider cannot change into its retry
/// queue forever, and a second account would give one person two personal
/// workspaces, which nothing downstream can tell apart.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_verified_signup_opens_one_account_and_a_replay_answers_with_it() {
    let lane = TestDatabase::shared();
    let database: Db = lane.open(DbRole::Api, &[]).await;
    let router = Fleet::live(
        database,
        "user_identity_signup_live",
        ScopeSet::from_scopes(&[]),
    )
    .with_identity_secret(SECRET)
    .router();

    // Minted per run so the case does not depend on the schema being reset
    // between them — `KEEP_TEST_STATE=1` is a supported inner loop, and a fixed
    // subject would make the second run of it fail as a replay of the first.
    let subject = format!("user_{}", mint_id().replace('-', ""));
    let address = format!("{subject}@example.test");
    let body = format!(
        r#"{{"type":"user.created","data":{{"id":"{subject}","email_addresses":[{{"id":"idn_1","email_address":"{address}"}}],"primary_email_address_id":"idn_1","first_name":"Ada","last_name":"Lovelace"}}}}"#
    );

    let opened = json_body(deliver(&router, &body).await).await;
    assert_eq!(
        opened.get("created").and_then(Value::as_bool),
        Some(true),
        "the first delivery of a subject nobody has seen opens the account"
    );
    let workspace = opened
        .get("workspace_id")
        .and_then(Value::as_str)
        .expect("an opened account names its workspace")
        .to_owned();
    assert!(
        !workspace.is_empty(),
        "an account with no workspace is one the person cannot reach"
    );

    let replayed = json_body(deliver(&router, &body).await).await;
    assert_eq!(
        replayed.get("created").and_then(Value::as_bool),
        Some(false),
        "a retry is a success carrying `created: false`, never an error"
    );
    assert_eq!(
        replayed.get("workspace_id").and_then(Value::as_str),
        Some(workspace.as_str()),
        "the replay names the workspace the first delivery opened — a second \
         one would give one person two personal workspaces"
    );
}

/// One correctly-signed delivery of `body` to a router the caller built.
///
/// Separate from [`signed`], which builds its own unreachable-pool fixture:
/// the live case needs the router to outlive the request so the same one takes
/// the replay.
async fn deliver(router: &axum::Router, body: &str) -> http::Response<axum::body::Body> {
    let signature = sign(DELIVERY, now(), body);
    send_with_headers(
        router,
        Method::POST,
        PATH,
        None,
        body,
        &headers(DELIVERY, &now().to_string(), &signature),
    )
    .await
}

/// A secret that is SET but will not parse is unconfigured, not a bad signature.
///
/// Two different refusals share one answer here on purpose, and the pairing is
/// the thing worth locking: an absent secret and an unreadable one both mean
/// nothing was checked, so neither can be reported as a verification that
/// failed. Calling a malformed secret a bad signature would send an operator
/// hunting the sender's key when the fault is this deployment's own
/// configuration.
#[tokio::test]
async fn a_secret_this_deployment_cannot_parse_is_unconfigured_rather_than_refused() {
    let router = Fleet::new()
        .with_identity_secret("this is not a vendor secret")
        .router();
    let signature = sign(DELIVERY, now(), CREATED);
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        CREATED,
        &headers(DELIVERY, &now().to_string(), &signature),
    )
    .await;

    assert_eq!(
        refusal_code(answer).await,
        code(error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED),
        "a secret that will not parse is this deployment's own configuration \
         failing, not the sender's signature"
    );
}

/// Every combination of the two name fields the provider may or may not send.
///
/// The provider sends either, both or neither, and the four cases are four
/// different stored values — the one that must not happen is a person stored
/// as `" Lovelace"` or `"Ada "` because an absent half was concatenated anyway.
/// Asserted against the column rather than the response, which does not echo
/// the name: a case that only ran the branch would pass while storing the
/// wrong string.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_name_the_provider_sends_only_half_of_is_stored_without_the_gap() {
    let lane = TestDatabase::shared();
    let database: Db = lane.open(DbRole::Api, &[]).await;
    let router = Fleet::live(
        database.clone(),
        "user_identity_signup_names",
        ScopeSet::from_scopes(&[]),
    )
    .with_identity_secret(SECRET)
    .router();

    for (given, family, expected) in [
        (Some("Ada"), Some("Lovelace"), Some("Ada Lovelace")),
        (None, Some("Lovelace"), Some("Lovelace")),
        (Some("Ada"), None, Some("Ada")),
        (None, None, None),
    ] {
        let subject = format!("user_{}", mint_id().replace('-', ""));
        let names = format!(
            r#""first_name":{},"last_name":{}"#,
            given.map_or("null".to_owned(), |it| format!(r#""{it}""#)),
            family.map_or("null".to_owned(), |it| format!(r#""{it}""#)),
        );
        let body = format!(
            r#"{{"type":"user.created","data":{{"id":"{subject}","email_addresses":[{{"id":"idn_1","email_address":"{subject}@example.test"}}],"primary_email_address_id":"idn_1",{names}}}}}"#
        );

        let answer = deliver(&router, &body).await;
        assert_eq!(
            answer.status(),
            StatusCode::OK,
            "given={given:?} family={family:?}"
        );

        let stored: Option<String> =
            sqlx::query_scalar("SELECT display_name FROM core.users WHERE oidc_subject = $1")
                .bind(&subject)
                .fetch_one(&mut *database.acquire().await.expect("a read connection"))
                .await
                .expect("the opened account is readable");

        assert_eq!(
            stored.as_deref(),
            expected,
            "given={given:?} family={family:?} must store {expected:?} — a \
             concatenation around an absent half stores a leading or trailing \
             space nobody typed"
        );
    }
}

/// The writeback the Rust port dropped.
///
/// Signup is TWO writes. The tenant row is the one this daemon owns; the second
/// tells the identity provider which tenant the account resolved to, and until
/// it lands the person's next session token carries no `tenant_id` — so every
/// call they make is refused for want of a tenant context. `identity_events_clerk.zig:290`
/// made that call and the Rust route did not, for the whole of the port: it
/// created tenants and told the provider nothing.
///
/// Nothing failed when it was missing, which is why this test exists rather
/// than a type. The write is best-effort by design — the row is already
/// committed, so a provider outage must not turn signup into a 500 — and an
/// omitted best-effort call produces no error, no 500, and no failing lane.
/// Only an assertion that it HAPPENED can see it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_verified_signup_tells_the_provider_which_tenant_it_opened() {
    let lane = TestDatabase::shared();
    let database: Db = lane.open(DbRole::Api, &[]).await;
    let fleet = Fleet::live(
        database,
        "user_identity_writeback_live",
        ScopeSet::from_scopes(&[]),
    )
    .with_identity_secret(SECRET);
    let writebacks = fleet.signup_writebacks();
    let router = fleet.router();

    let subject = format!("user_{}", mint_id().replace('-', ""));
    let address = format!("{subject}@example.test");
    let body = format!(
        r#"{{"type":"user.created","data":{{"id":"{subject}","email_addresses":[{{"id":"idn_1","email_address":"{address}"}}],"primary_email_address_id":"idn_1","first_name":"Ada","last_name":"Lovelace"}}}}"#
    );

    let opened = json_body(deliver(&router, &body).await).await;
    assert_eq!(
        opened.get("created").and_then(Value::as_bool),
        Some(true),
        "the case needs a fresh account, not a replay"
    );

    let written = writebacks.written();
    assert_eq!(
        written.len(),
        1,
        "one account opened is one writeback — a second would mean the handler \
         wrote on the replay path too"
    );
    let Some(wrote) = written.first() else {
        // Unreachable past the length assertion; spelled as a fallible read
        // because this crate's suites index nothing.
        return;
    };
    assert_eq!(
        wrote.subject, subject,
        "the write addresses the subject the event named — an account repaired \
         under a different one is a different person's"
    );
    assert!(
        !wrote.tenant_id.is_empty(),
        "a writeback carrying no tenant is the bug it exists to prevent: the \
         provider merges an empty claim and the next token still has none"
    );
    assert_eq!(
        wrote.scopes,
        afd_auth::scope::signup_owner_claim(),
        "the owner grant is what makes the account's first workspace usable; \
         `signup_owner_claim` had NO production caller before this write"
    );
}

/// The provider refusing the writeback does not refuse the delivery.
///
/// The tenant row is already committed when the write runs, so a refusal there
/// answered to the provider would refuse an account that exists and invite a
/// retry that can only duplicate work. The handler swallows it and LOGS it,
/// and this case proves the swallow from both sides: the seam refuses every
/// write, the delivery still answers 200 with the account it opened, and the
/// seam shows the write was ATTEMPTED — a handler that skipped the call would
/// pass a status-only assertion while leaving the operator nothing to repair
/// from.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_provider_that_will_not_take_the_writeback_does_not_refuse_the_delivery() {
    let lane = TestDatabase::shared();
    let database: Db = lane.open(DbRole::Api, &[]).await;
    let refusing = RecordingWriteback::refusing(MetadataUnwritten::Unauthorized);
    let router = Fleet::live(
        database,
        "user_identity_writeback_refused",
        ScopeSet::from_scopes(&[]),
    )
    .with_identity_secret(SECRET)
    .with_signup_writeback(refusing.clone())
    .router();

    let subject = format!("user_{}", mint_id().replace('-', ""));
    let address = format!("{subject}@example.test");
    let body = format!(
        r#"{{"type":"user.created","data":{{"id":"{subject}","email_addresses":[{{"id":"idn_1","email_address":"{address}"}}],"primary_email_address_id":"idn_1"}}}}"#
    );

    let answer = deliver(&router, &body).await;
    assert_eq!(
        answer.status(),
        StatusCode::OK,
        "the row is committed before the write runs; a refused write must not \
         turn an opened account into a delivery the provider retries"
    );
    let opened = json_body(answer).await;
    assert_eq!(
        opened.get("created").and_then(Value::as_bool),
        Some(true),
        "the case needs a fresh account, not a replay"
    );
    assert_eq!(
        refusing.written().len(),
        1,
        "the write was attempted and refused — a handler that never asked \
         would be a skipped write wearing a swallowed one's clothes"
    );
}

/// A subject the account model tolerated and the write cannot address.
///
/// `bootstrap` opens an account under whatever subject the provider sent: the
/// column is `TEXT NOT NULL`, and a run of spaces satisfies it. The writeback
/// cannot follow — a blank subject resolves to nobody at the provider — so the
/// handler declines the write rather than asking the provider to merge a claim
/// into no one. The delivery is still answered, because the row is committed;
/// what the operator has is the log line. Proven by the seam recording
/// NOTHING rather than by a status, since a status alone cannot tell a
/// declined write from one that was never reached.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_subject_that_is_only_whitespace_opens_the_account_but_is_not_written_back() {
    let lane = TestDatabase::shared();
    let database: Db = lane.open(DbRole::Api, &[]).await;
    let fleet = Fleet::live(
        database,
        "user_identity_writeback_blank",
        ScopeSet::from_scopes(&[]),
    )
    .with_identity_secret(SECRET);
    let writebacks = fleet.signup_writebacks();
    let router = fleet.router();

    // The subject is fixed — it is the whole point — and the address is minted,
    // so a `KEEP_TEST_STATE=1` rerun replays the same account rather than
    // colliding on a second one. The replay path reaches the same write.
    let address = format!("blank-{}@example.test", mint_id().replace('-', ""));
    let body = format!(
        r#"{{"type":"user.created","data":{{"id":"   ","email_addresses":[{{"id":"idn_1","email_address":"{address}"}}],"primary_email_address_id":"idn_1"}}}}"#
    );

    let answer = deliver(&router, &body).await;
    assert_eq!(
        answer.status(),
        StatusCode::OK,
        "the account model took the subject, so the delivery is answered; the \
         gap is the provider's to see in the log, not a refusal to retry"
    );
    assert!(
        writebacks.written().is_empty(),
        "a blank subject addresses nobody — the write must be declined, not \
         sent for the provider to merge a claim into no one"
    );
}
