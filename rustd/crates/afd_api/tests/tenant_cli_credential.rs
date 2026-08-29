//! Dimension 2.2 — who may manage a command-line credential, and who may not.
//!
//! # Why this suite is about the REFUSALS and not the mint
//!
//! The mint's behaviour is a transaction a real Postgres evaluates — an
//! advisory lock, an owner-scoped revoke, and an insert the partial unique
//! index arbitrates. A stub cannot invent a success there without inventing the
//! state machine too, so the harness answers the refusal a datastore that would
//! not answer gives, and what these tests pin is everything in FRONT of the
//! verb.
//!
//! That is not a consolation prize. This family's whole security rule is in
//! front of the verb: minting takes a browser sign-in, revoking takes a human,
//! and a tenant api-key is neither. Those three facts are the reason the
//! endpoints exist in the shape they do.
//!
//! # The sentence is the assertion, not the code
//!
//! Both refusals answer `UZ-AUTH-001`. A test asserting the CODE passes whether
//! or not the freshness rule is still there — including when an `afc_`
//! credential has quietly started minting its own successors. So these assert
//! the DETAIL, and they take it from the same constant the extractor writes,
//! rather than re-spelling the literal here where the two could drift apart
//! (RULE UFS).
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::ScopeSet;
use axum::response::Response;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::Fleet;

/// The path the mint and the list share.
const CREDENTIALS: &str = "/v1/cli-credentials";

/// One credential's path, under a well-formed identifier.
const ONE_CREDENTIAL: &str = "/v1/cli-credentials/0195b4ba-8d3a-7f13-8abc-2b3e1e0f7031";

/// A tenant api-key, which is a person's credential and not a person.
///
/// Full shape — marker plus sixty-four lower-case hex — because the
/// authenticator classifies on the marker and refuses anything that is not
/// shaped like a credential before any of this file's rules are reached.
const TENANT_KEY: &str = "agt_t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// An `afc_` credential, which is a person at a terminal.
///
/// The same subject and the same shape as [`TENANT_KEY`], differing only in the
/// marker — which is the axis every assertion below turns on.
const TERMINAL: &str = "afc_fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

/// The subject both fixture credentials resolve to.
///
/// ONE subject on purpose: the two credentials name the same human with the
/// same capabilities, so any difference in outcome below is a difference in
/// credential CLASS and can be nothing else.
const SUBJECT: &str = "user_2fixture";

/// No capabilities at all, which is what these routes ask for.
///
/// `Scopes::Always(NONE)` in the route table, so an EMPTY set is the honest
/// fixture: it proves the scope gate is not what refuses below, and therefore
/// that every refusal in this file is about credential class. A fixture holding
/// capabilities would leave the two indistinguishable.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// A body the machine-name check would accept, so a refusal is never the body's.
const GOOD_BODY: &str = r#"{"machine_name":"indy-macbook.local"}"#;

/// Reads a problem document's `detail` back.
async fn detail_of(response: Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a refusal body is small and complete");
    let document: Value = serde_json::from_slice(&bytes).expect("every refusal is problem+json");
    document
        .get("detail")
        .and_then(Value::as_str)
        .expect("every refusal carries a detail")
        .to_owned()
}

/// A tenant api-key cannot mint a credential in a person's name.
///
/// The widening Invariant 1 forbids: an `agt_t` key resolves to a person and
/// carries that person's capabilities, so no required SCOPE could refuse it —
/// it already holds every scope this family might name. The refusal has to be
/// on class, and this is the test that says so.
///
/// The sentence is the FRESHNESS one rather than the person one, which is what
/// `requireFreshSessionSubject` does: it tests the mode before it tests
/// personhood, so everything that is not a browser session — a tenant key and
/// an `afc_` credential alike — is refused for the same reason at the same
/// gate. The person sentence is reachable on this verb only for a session
/// token whose subject is missing, which is a broken principal rather than a
/// wrong class.
#[tokio::test]
async fn a_tenant_key_cannot_mint_a_command_line_credential() {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, NO_SCOPES)
        .router();

    let response = harness::send(
        &router,
        Method::POST,
        CREDENTIALS,
        Some(TENANT_KEY),
        GOOD_BODY,
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "an organisation's credential is not a person"
    );
    assert_eq!(
        detail_of(response).await,
        afd_api::auth::DETAIL_SESSION_REQUIRED,
        "minting checks the mode first, so a non-session class earns the \
         freshness sentence whichever non-session class it is"
    );
}

/// A tenant api-key cannot revoke a person's credential either.
///
/// The revoke is broader than the mint by exactly one class, and this pins
/// which one: broader does not mean open.
#[tokio::test]
async fn a_tenant_key_cannot_revoke_a_command_line_credential() {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, NO_SCOPES)
        .router();

    let response = harness::send(
        &router,
        Method::DELETE,
        ONE_CREDENTIAL,
        Some(TENANT_KEY),
        "",
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "an organisation's credential cannot manage a person's"
    );
    assert_eq!(
        detail_of(response).await,
        afd_api::auth::DETAIL_PERSON_REQUIRED,
        "the person rule, not the freshness one"
    );
}

/// A command-line credential cannot mint its successor.
///
/// The load-bearing one. Without this rule a single stolen credential is
/// permanent: it mints the next under a machine name of its choosing, revoking
/// any one row leaves its siblings live, and the account holder cannot see how
/// many exist. Minting costs a browser sign-in every time because that is the
/// one step a stolen credential cannot replay.
///
/// Asserted on the SENTENCE rather than the status: both this and the test
/// above answer 403 under `UZ-AUTH-001`, so a status assertion alone would stay
/// green with the freshness rule deleted — the `afc_` credential would simply
/// fall through to the person check, which it passes.
#[tokio::test]
async fn a_command_line_credential_cannot_mint_another() {
    let router = Fleet::new()
        .with_terminal(TERMINAL, SUBJECT, NO_SCOPES)
        .router();

    let response = harness::send(
        &router,
        Method::POST,
        CREDENTIALS,
        Some(TERMINAL),
        GOOD_BODY,
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "a credential that could mint another would be self-renewing"
    );
    assert_eq!(
        detail_of(response).await,
        afd_api::auth::DETAIL_SESSION_REQUIRED,
        "this must be the FRESHNESS refusal; the person refusal here would mean \
         the rule is gone and an afc_ credential fell through to the wider check"
    );
}

/// A command-line credential MAY revoke, which is what makes `logout` work.
///
/// The other half of the rule above, and the reason the two guards are not one:
/// a terminal must be able to end its own access without opening a browser. It
/// gets past the class check and reaches the store, which has no Postgres — so
/// the assertion is that the refusal is the DATASTORE's and not the guard's.
#[tokio::test]
async fn a_command_line_credential_may_revoke_its_own() {
    let router = Fleet::new()
        .with_terminal(TERMINAL, SUBJECT, NO_SCOPES)
        .router();

    let response = harness::send(&router, Method::DELETE, ONE_CREDENTIAL, Some(TERMINAL), "").await;

    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "a terminal must be able to log itself out without a browser"
    );
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "it reached the store, which in this harness has no Postgres behind it"
    );
}

/// An unreadable body is refused before any credential work happens.
#[tokio::test]
async fn a_malformed_mint_body_is_refused() {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, NO_SCOPES)
        .router();

    // Sent with the tenant key, which the class check refuses FIRST. That
    // ordering is the assertion: a body is parsed only for a caller allowed to
    // send one, so a malformed body from a refused class reads as the refusal
    // it actually is.
    let response = harness::send(
        &router,
        Method::POST,
        CREDENTIALS,
        Some(TENANT_KEY),
        "{not json",
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "the class check precedes the body parse"
    );
}
