//! What `/v1/connectors/{provider}/callback` refuses, and where in the order.
//!
//! One path, two routes, two guards. `GET` is where the PROVIDER sends the
//! browser, so it carries no credential of ours and is `Guard::Open`; it reads
//! nothing and writes nothing, and only redirects to the dashboard's own relay.
//! `POST` is the dashboard coming back with the person's bearer, and it is the
//! only endpoint in this family that redeems a code or writes a connection.
//!
//! # These run with no datastore, and that is a property of the seam
//!
//! `Fleet`'s connect flow is the PRODUCTION one over stores that are not there
//! and a vendor nothing resolves, so every refusal here is the one
//! `afd_connector` actually raises rather than one a mock was told to raise. A
//! case that stopped refusing would reach an unreachable store and fail loudly
//! instead of passing quietly.
//!
//! # Why the boundary of this file is exactly where it is
//!
//! `complete` refuses four times before it reads the state signing secret out
//! of the vault: an unshipped provider, a callback with no state, one with no
//! code, and a deployment with no platform admin workspace. Those four are
//! here. Everything past that point — a forged, expired, spent or foreign
//! state, which is the rest of Dimension 4.2 — needs a vault the fixture cannot
//! serve, and belongs to the integration lane. Splitting there is deliberate:
//! it is the last line at which the handler has touched no store.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use crate::harness;

use self::harness::Fleet;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code::{self, ErrorCode};
use http::{Method, StatusCode, header};

/// A person holding the connector-write scope `Complete` demands.
const TENANT_KEY: &str = "agt_tdecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbad";

/// The subject that key authenticates as.
const SUBJECT: &str = "user_connecting";

/// A provider this daemon ships a connector for.
const SHIPPED: &str = "slack";

/// A provider it does not, spelled as a person might guess it.
const UNSHIPPED: &str = "dropbox";

/// An opaque state, shaped like one this daemon would sign but not signed.
const FORGED_STATE: &str = "not-a-state-this-daemon-signed";

/// The authorization code a provider hands back.
const CODE: &str = "vendor-authorization-code";

/// Where the dashboard lives, as the fixture configures it.
const DASHBOARD: &str = "https://app.fixture.test";

fn path(provider: &str) -> String {
    format!("/v1/connectors/{provider}/callback")
}

/// The registry code, as it is spelled on the wire.
fn code(code: ErrorCode) -> String {
    code.as_str().to_owned()
}

/// One request at a fresh router, as the browser arrives — no credential.
async fn browser(provider: &str, query: &str) -> axum::response::Response {
    let router = Fleet::new().router();
    let target = format!("{}?{query}", path(provider));
    harness::send(&router, Method::GET, &target, None, "").await
}

/// One request at a fresh router, as the dashboard returns — bearing a person.
async fn dashboard(provider: &str, query: &str) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(
            TENANT_KEY,
            SUBJECT,
            ScopeSet::from_scopes(&[Scope::ConnectorWrite]),
        )
        .router();
    let target = format!("{}?{query}", path(provider));
    harness::send(&router, Method::POST, &target, Some(TENANT_KEY), "").await
}

/// The registry code a response carries, asserting it refused at all.
async fn refusal(response: axum::response::Response) -> String {
    let status = response.status();
    let document = harness::json_body(response).await;
    assert!(
        status.is_client_error(),
        "a refusal is the caller's to fix, so it is a 4xx: {status} {document}"
    );
    document
        .get("error_code")
        .and_then(serde_json::Value::as_str)
        .expect("every refusal carries its registry code")
        .to_owned()
}

// ── GET: the browser leg, which touches nothing ──────────────────────────────

#[tokio::test]
async fn the_browser_leg_hands_the_whole_handoff_to_the_dashboard() {
    let response = browser(SHIPPED, &format!("code={CODE}&state={FORGED_STATE}")).await;

    assert_eq!(
        response.status(),
        StatusCode::FOUND,
        "the provider sent a browser here; it leaves with a redirect"
    );
    let destination = response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .expect("a redirect names where it goes")
        .to_owned();

    assert!(
        destination.starts_with(DASHBOARD),
        "the relay goes to the dashboard, not back to the vendor: {destination}"
    );
    assert!(
        destination.contains(CODE) && destination.contains(FORGED_STATE),
        "the dashboard cannot complete a connect it was not handed the code and \
         state for: {destination}"
    );
}

#[tokio::test]
async fn the_browser_leg_verifies_nothing_and_that_is_the_point() {
    let response = browser(SHIPPED, &format!("code={CODE}&state={FORGED_STATE}")).await;

    assert_eq!(
        response.status(),
        StatusCode::FOUND,
        "an unsigned state still relays: this leg carries no credential and \
         redeems nothing, so refusing here would only move the refusal earlier \
         for a person who would be refused at POST anyway"
    );
}

#[tokio::test]
async fn a_browser_arriving_with_no_state_is_refused() {
    assert_eq!(
        refusal(browser(SHIPPED, &format!("code={CODE}")).await).await,
        code(error_code::INVALID_REQUEST),
        "the state is what the dashboard completes with; a relay without it \
         would send a person to a page that cannot finish"
    );
}

#[tokio::test]
async fn a_provider_this_daemon_ships_nothing_for_is_refused_on_the_browser_leg() {
    assert_eq!(
        refusal(browser(UNSHIPPED, &format!("code={CODE}&state={FORGED_STATE}")).await).await,
        code(error_code::CONNECTOR_UNKNOWN)
    );
}

#[tokio::test]
async fn a_query_this_daemon_cannot_decode_is_refused_rather_than_relayed() {
    assert_eq!(
        refusal(browser(SHIPPED, "state=%ZZ").await).await,
        code(error_code::INVALID_REQUEST),
        "a percent-escape that is not one must not be forwarded into the \
         dashboard's URL, where it would be a second parser's problem"
    );
}

// ── POST: the dashboard leg, up to its first store read ──────────────────────

#[tokio::test]
async fn a_provider_this_daemon_ships_nothing_for_is_refused_before_anything_else() {
    assert_eq!(
        refusal(dashboard(UNSHIPPED, &format!("code={CODE}&state={FORGED_STATE}")).await).await,
        code(error_code::CONNECTOR_UNKNOWN)
    );
}

#[tokio::test]
async fn a_completion_carrying_no_state_is_refused() {
    assert_eq!(
        refusal(dashboard(SHIPPED, &format!("code={CODE}")).await).await,
        code(error_code::INVALID_REQUEST)
    );
}

#[tokio::test]
async fn a_completion_carrying_no_code_is_refused() {
    assert_eq!(
        refusal(dashboard(SHIPPED, &format!("state={FORGED_STATE}")).await).await,
        code(error_code::INVALID_REQUEST),
        "there is nothing to exchange without one, and the refusal names the \
         missing parameter rather than failing at the vendor"
    );
}

#[tokio::test]
async fn the_dashboard_leg_demands_a_credential_where_the_browser_leg_does_not() {
    let router = Fleet::new()
        .with_person(
            TENANT_KEY,
            SUBJECT,
            ScopeSet::from_scopes(&[Scope::ConnectorWrite]),
        )
        .router();
    let target = format!("{}?code={CODE}&state={FORGED_STATE}", path(SHIPPED));
    let response = harness::send(&router, Method::POST, &target, None, "").await;

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "one path, two guards: `Guard::Open` on GET and `Guard::Bearer` on \
         POST. A router mounting one guard for the path would either lock the \
         provider out or let anyone redeem a code."
    );
}

#[tokio::test]
async fn a_person_without_the_connector_write_scope_cannot_complete() {
    let router = Fleet::new()
        .with_person(
            TENANT_KEY,
            SUBJECT,
            ScopeSet::from_scopes(&[Scope::ConnectorRead]),
        )
        .router();
    let target = format!("{}?code={CODE}&state={FORGED_STATE}", path(SHIPPED));
    let response = harness::send(&router, Method::POST, &target, Some(TENANT_KEY), "").await;

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "reading which connectors exist and redeeming a grant into the vault \
         are different capabilities"
    );
}
