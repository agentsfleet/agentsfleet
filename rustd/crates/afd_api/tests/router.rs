//! The router: what it serves, what it refuses, and what it does not claim.
//!
//! The assertion that matters most here is a NEGATIVE one. axum answers HEAD
//! with the GET handler unless something stops it, and the scope table
//! resolves an unnamed method to the WRITE rung — so a HEAD that reached the
//! router would be answered by a read handler behind a write gate. axum makes
//! that live by default, which is why the router refuses HEAD outright.
//!
//! What the two probes report is `router_probes.rs`, which sends through the
//! helper here.
#![cfg(feature = "test-util")]

use crate::harness;

use afd_api::route::Route;
use afd_api::router::ReadyInputs;
use axum::response::Response;
use http::{Method, StatusCode};

use self::harness::Fleet;

/// Sends one request at a router whose dependencies report `inputs`.
///
/// No credential, deliberately. Every assertion in this suite is about ROUTING
/// — which paths exist, which methods they answer, and what an unmounted one
/// does — and a guarded route answers those questions from its refusal exactly
/// as well as from its handler. The credential matrix is `runner_plane/mod.rs`.
pub(crate) async fn send(method: Method, path: &str, inputs: ReadyInputs) -> Response {
    let router = Fleet::new().reporting(inputs).router();
    harness::send(&router, method, path, None, "").await
}

/// Every dependency reachable.
pub(crate) const ALL_HEALTHY: ReadyInputs = ReadyInputs {
    database: true,
    queue: true,
};

/// HEAD is refused, not answered by the GET handler behind it.
#[tokio::test]
async fn test_head_is_refused_on_a_route_that_serves_get() {
    let refused = send(Method::HEAD, "/healthz", ALL_HEALTHY).await;

    assert_eq!(
        refused.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "axum answers HEAD from the GET handler unless the router stops it"
    );
}

/// Every tabled template with no path parameter resolves through the full
/// router.
///
/// Not a 200: a mounted route answers its guard's refusal, or a 405 when it
/// serves a method other than the GET sent here. What is asserted is that the
/// PATH resolves through the layered router, which `route_verbs.rs` cannot see
/// because it probes each route's unlayered mount. A parameterised template is
/// skipped because it would have to be filled in to be requested at all.
#[tokio::test]
async fn test_every_parameterless_template_resolves() {
    for route in Route::all() {
        let template = route.meta().template;
        if template.contains('{') {
            continue;
        }
        let response = send(Method::GET, template, ALL_HEALTHY).await;
        assert_ne!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{template} is tabled, so the router must serve it"
        );
    }
}

/// The connect completion's path has no open door.
///
/// It redeems an authorization code and writes a connection, so the bearer
/// guard covers the whole path: an unproven caller is turned away before any
/// method is matched, whichever method it sends.
#[tokio::test]
async fn test_the_connect_completion_path_is_bearer_guarded_for_every_method() {
    const PATH: &str = "/v1/connectors/github/callback";

    for method in [Method::POST, Method::GET] {
        let unproven = send(method.clone(), PATH, ALL_HEALTHY).await;
        assert_eq!(unproven.status(), StatusCode::UNAUTHORIZED, "{method}");
    }
}

/// The ownership layer over a router assembled by hand: a route whose path
/// carries no workspace, and one whose guard was never mounted. Both are
/// this daemon's wiring mistake, so both fail closed as internal faults
/// rather than as a caller's missing or foreign workspace.
#[tokio::test]
async fn an_ownership_layer_mounted_by_hand_fails_closed() {
    use afd_http::auth::{Owner, own};
    use afd_http::route::{Guard, RouteClass, RouteMeta, Scopes};
    use std::sync::Arc;

    const TEMPLATE: &str = "/v1/workspaces/{workspace_id}/probe";
    let owner = Owner::new(
        Arc::new(Fleet::new()),
        RouteMeta::new(
            Guard::Bearer,
            RouteClass::Api,
            TEMPLATE,
            Scopes::Always(&[]),
        ),
    );
    let router = axum::Router::new()
        .route("/probe", axum::routing::get(|| async { "served" }))
        .route(TEMPLATE, axum::routing::get(|| async { "served" }))
        .layer(axum::middleware::from_fn_with_state(owner, own::<Fleet>));

    for path in [
        "/probe",
        "/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d02/probe",
    ] {
        let response = harness::send(&router, Method::GET, path, None, "").await;
        assert_eq!(
            response.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "{path}"
        );
        assert_eq!(
            harness::code_of(response).await,
            afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str(),
            "{path}"
        );
    }
}
