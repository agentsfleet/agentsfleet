//! A member of an account is refused, inside it, exactly the routes only its
//! owner may use, and nothing else.
//!
//! Datastore-free. The ownership stub decides the caller's role, so the walk
//! covers every mounted workspace route and method in milliseconds. What it
//! proves is the PLACEMENT of the refusal: on each route whose method needs
//! `secret:write` or `connector:write`, and on no other. That the role is read
//! from a real membership row is the live suite's claim, not this one's.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_api::Route;
use afd_api::route::{RouteClass, WorkspaceRoute};
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use axum::Router;
use axum::response::Response;
use http::StatusCode;
use serde_json::Value;

use self::harness::{ERROR_CODE, Fleet, OWNED_WORKSPACE, concrete_path, send};

const TERMINAL: &str = "afc_0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e";
const SUBJECT: &str = "user_2member_roles";

/// The capabilities the role withholds from a member.
const OWNER_ONLY: [Scope; 2] = [Scope::SecretWrite, Scope::ConnectorWrite];

/// Every workspace route and method, the path it is sent to, and whether its
/// requirement is owner-grade.
///
/// Streams are left out: a served stream never finishes its body, and none of
/// them requires an owner-grade capability, which the walk asserts rather than
/// assumes.
fn workspace_requests() -> Vec<(String, http::Method, bool)> {
    let mut requests = Vec::new();
    for route in Route::all() {
        let meta = route.meta();
        if !meta.ownership.is_checked() {
            continue;
        }
        for verb in route.verbs() {
            let method = verb.method();
            let owner_only = meta
                .scopes
                .required(&method)
                .iter()
                .any(|scope| OWNER_ONLY.contains(scope));
            if meta.class == RouteClass::Stream {
                assert!(!owner_only, "{} is a stream and owner-only", meta.template);
                continue;
            }
            let path = concrete_path(meta.template, Some(OWNED_WORKSPACE));
            requests.push((path, method, owner_only));
        }
    }
    requests
}

/// A router whose caller holds every scope and opens the owned workspace.
fn router(as_member: bool) -> Router {
    let fleet = Fleet::new().with_terminal(TERMINAL, SUBJECT, ScopeSet::from_scopes(&Scope::ALL));
    if as_member {
        fleet.ownership().join_as_member();
    }
    fleet.router()
}

/// The registry code a response carries, when its body is a problem.
async fn code_of(response: Response) -> Option<String> {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a test response body is small and in memory");
    serde_json::from_slice::<Value>(&bytes)
        .ok()?
        .get(ERROR_CODE)?
        .as_str()
        .map(str::to_owned)
}

/// Dimension 2.3: a member reaches every workspace route except the
/// owner-grade ones, and those answer `403 UZ-AUTH-026`.
#[tokio::test]
async fn test_member_refused_owner_only_routes() {
    let router = router(true);
    let requests = workspace_requests();
    let owner_grade = requests.iter().filter(|(_, _, only)| *only).count();
    assert!(
        owner_grade >= 4,
        "the walk found {owner_grade} owner-grade requests; secrets and connectors mount more"
    );

    for (path, method, owner_only) in requests {
        let response = send(&router, method.clone(), &path, Some(TERMINAL), "{}").await;
        let status = response.status();
        let code = code_of(response).await;
        let refused_as_member = code.as_deref() == Some(error_code::AUTH_OWNER_ONLY.as_str());
        if owner_only {
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
            assert!(refused_as_member, "{method} {path} answered {code:?}");
        } else {
            assert!(!refused_as_member, "{method} {path} withheld from a member");
        }
    }
}

/// The owner of the same account is never refused as a member, on any route.
#[tokio::test]
async fn test_an_owner_is_never_refused_as_a_member() {
    let router = router(false);
    for (path, method, _) in workspace_requests() {
        let response = send(&router, method.clone(), &path, Some(TERMINAL), "{}").await;
        let code = code_of(response).await;
        assert_ne!(
            code.as_deref(),
            Some(error_code::AUTH_OWNER_ONLY.as_str()),
            "{method} {path} refused an owner"
        );
    }
}

/// Dimension 2.5: an access check that cannot reach Postgres answers `503`,
/// never `403`.
///
/// The production resolver over a pool nobody listens on, which is the
/// outage itself rather than a stub imitating one. Answering "not yours" here
/// would tell a person their workspace had vanished during a blip.
#[tokio::test]
async fn test_access_check_outage_is_not_denial() {
    let router = Fleet::new()
        .with_terminal(TERMINAL, SUBJECT, ScopeSet::from_scopes(&Scope::ALL))
        .with_live_ownership()
        .router();
    let path = concrete_path(
        WorkspaceRoute::Fleets.meta().template,
        Some(OWNED_WORKSPACE),
    );

    let response = send(&router, http::Method::GET, &path, Some(TERMINAL), "").await;
    let status = response.status();
    let code = code_of(response).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        code.as_deref(),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
}
