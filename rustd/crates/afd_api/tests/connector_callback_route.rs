//! What `/v1/connectors/{provider}/callback` refuses, and where in the order.
//!
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
use afd_core::error_code;
use afd_core::id::Uuid7;
use http::{Method, StatusCode};
use serde_json::Value;

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

fn path(provider: &str) -> String {
    format!("/v1/connectors/{provider}/callback")
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

// ── POST: the dashboard leg, up to its first store read ──────────────────────

#[tokio::test]
async fn a_provider_this_daemon_ships_nothing_for_is_refused_before_anything_else() {
    assert_eq!(
        harness::refusal_code(
            dashboard(UNSHIPPED, &format!("code={CODE}&state={FORGED_STATE}")).await
        )
        .await,
        error_code::CONNECTOR_UNKNOWN.as_str()
    );
}

#[tokio::test]
async fn a_completion_carrying_no_state_is_refused() {
    assert_eq!(
        harness::refusal_code(dashboard(SHIPPED, &format!("code={CODE}")).await).await,
        error_code::INVALID_REQUEST.as_str()
    );
}

#[tokio::test]
async fn a_completion_carrying_no_code_is_refused() {
    assert_eq!(
        harness::refusal_code(dashboard(SHIPPED, &format!("state={FORGED_STATE}")).await).await,
        error_code::INVALID_REQUEST.as_str(),
        "there is nothing to exchange without one, and the refusal names the \
         missing parameter rather than failing at the vendor"
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

/// A deployment with no admin workspace cannot connect anything, and says so.
///
/// The admin workspace is where this deployment's own platform credentials
/// live, so without one there is no app bag to start a connect from, no secret
/// to sign the state with, and nothing for the rest of the handler to do. The
/// check sits FIRST for that reason — ahead of the secret read and the nonce —
/// and it is the cheapest of the three refusals that answer `UZ-CONN-001`.
///
/// Reaching this needs a caller who is otherwise entirely in order: a real
/// bearer, the write scope, and a workspace they own. That is the point. The
/// refusal is not about them, which is why it is a 503 and not a 4xx, and why
/// getting it wrong would tell an operator's user that their own request was
/// malformed when the deployment is simply not set up.
#[tokio::test]
async fn a_deployment_with_no_admin_workspace_cannot_start_a_connect() {
    let workspace = Uuid7::parse("0199a0b0-0000-7000-8000-0000000000c1")
        .expect("the fixture workspace is canonical");
    let router = Fleet::new()
        .with_person(
            TENANT_KEY,
            SUBJECT,
            ScopeSet::from_scopes(&[Scope::ConnectorWrite]),
        )
        .with_owned_workspace(workspace.clone())
        .router();

    let target = format!(
        "/v1/workspaces/{}/connectors/{SHIPPED}/connect",
        workspace.as_str()
    );
    let response = harness::send(&router, Method::POST, &target, Some(TENANT_KEY), "").await;

    let status = response.status();
    let document = harness::json_body(response).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "the caller is in order; the deployment is not: {document}"
    );
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::CONNECTOR_NOT_CONFIGURED.as_str()),
        "a deployment holding no admin workspace can configure no app, and \
         answers that rather than a refusal about the caller"
    );
}
