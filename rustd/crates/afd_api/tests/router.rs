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
/// as well as from its handler. The credential matrix is `runner_plane.rs`.
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

/// One template, two guards — and the merge must not flatten them to one.
///
/// `/v1/connectors/{provider}/callback` is the provider's unauthenticated
/// redirect on GET and the dashboard's bearer-proven completion on POST. The
/// mount loop merges same-template routes into one `MethodRouter` because axum
/// takes one per path, and merging BEFORE layering would put a single guard on
/// the merged pair — whichever route the loop reached first. `ConnectorRoute`'s
/// roster reaches `Callback` before `Complete`, so that guard is the OPEN one,
/// and the endpoint that redeems an authorization code and writes a connection
/// would have answered anyone who found the URL.
///
/// Two assertions rather than one. The POST alone would also pass on a router
/// that had lost the OPEN guard instead — bearer everywhere is safe and wrong —
/// so the GET is asserted beside it, and together they say the two guards
/// survived the merge as two.
#[tokio::test]
async fn test_the_callback_pair_keeps_a_guard_each_through_the_merge() {
    const PATH: &str = "/v1/connectors/github/callback";

    let unproven = send(Method::POST, PATH, ALL_HEALTHY).await;
    assert_eq!(
        unproven.status(),
        StatusCode::UNAUTHORIZED,
        "the completion redeems a code and writes a connection: it is bearer-guarded, \
         and merging it with the open relay must not surrender that"
    );

    // Sent with no `state`, so the relay refuses on its own terms — which is
    // the assertion: reaching a handler's OWN refusal proves the request got
    // past the guard rather than being turned away in front of it.
    let relayed = send(Method::GET, PATH, ALL_HEALTHY).await;
    assert_ne!(
        relayed.status(),
        StatusCode::UNAUTHORIZED,
        "the provider's browser carries no credential of ours, so the relay stays open"
    );
}
