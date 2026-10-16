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
