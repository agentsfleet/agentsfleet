//! The router: what it serves, what it refuses, and what it does not claim.
//!
//! The assertion that matters most here is a NEGATIVE one. axum answers HEAD
//! with the GET handler unless something stops it, and the Zig scope table
//! resolves an unnamed method to the WRITE rung — so a HEAD that reached the
//! router would have been answered by a read handler behind a write gate.
//! Dormant in Zig because the request never arrives; live in axum by default.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod harness;

use afd_api::route::{OpsRoute, Route, RunnerOpsRoute, RunnerRoute};
use afd_api::router::{ReadyInputs, ready_decision};
use axum::response::Response;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::Fleet;

/// Sends one request at a router whose dependencies report `inputs`.
///
/// No credential, deliberately. Every assertion in this suite is about ROUTING
/// — which paths exist, which methods they answer, and what an unmounted one
/// does — and a guarded route answers those questions from its refusal exactly
/// as well as from its handler. The credential matrix is `runner_plane.rs`.
async fn send(method: Method, path: &str, inputs: ReadyInputs) -> Response {
    let router = Fleet::new().reporting(inputs).router();
    harness::send(&router, method, path, None, "").await
}

/// Every dependency reachable.
const ALL_HEALTHY: ReadyInputs = ReadyInputs {
    database: true,
    queue: true,
};

/// Reads a response body back as JSON.
async fn json_body(response: Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a probe body is small and in memory");
    serde_json::from_slice(&bytes).expect("a probe answers JSON")
}

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

/// A HEAD at a path this binary does not serve is still a 404.
///
/// The refusal is mounted with `route_layer` for this: a 405 would say the
/// path exists and only the method is wrong, which for an unmounted route is
/// not true.
#[tokio::test]
async fn test_head_at_an_unserved_path_is_not_found() {
    let missing = send(Method::HEAD, "/v1/workspaces", ALL_HEALTHY).await;

    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

/// Liveness answers for the process and says nothing about dependencies.
#[tokio::test]
async fn test_healthz_is_liveness_only() {
    let alive = send(
        Method::GET,
        "/healthz",
        ReadyInputs {
            database: false,
            queue: false,
        },
    )
    .await;

    assert_eq!(
        alive.status(),
        StatusCode::OK,
        "a dependency outage must not make liveness flap — that gets the \
         process killed, which does nothing about the dependency"
    );

    let body = json_body(alive).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "agentsfleetd");
    assert!(body.get("version").is_some(), "the build is reported");
    assert!(body.get("commit").is_some(), "the commit is reported");
    assert!(
        body.get("database").is_none() && body.get("queue").is_none(),
        "the dependency fields were deliberately dropped from /healthz — \
         liveness does not probe, and a merge must not put them back"
    );
}

/// Readiness reports each dependency separately, and answers 200 when all are up.
#[tokio::test]
async fn test_readyz_is_green_when_every_dependency_answers() {
    let ready = send(Method::GET, "/readyz", ALL_HEALTHY).await;
    assert_eq!(ready.status(), StatusCode::OK);

    let body = json_body(ready).await;
    assert_eq!(body["ready"], true);
    assert_eq!(body["database"], true);
    assert_eq!(body["queue"], true);
}

/// One red dependency takes the instance out of rotation, and names itself.
#[tokio::test]
async fn test_readyz_is_red_and_says_which_dependency() {
    let degraded = send(
        Method::GET,
        "/readyz",
        ReadyInputs {
            database: true,
            queue: false,
        },
    )
    .await;

    assert_eq!(
        degraded.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "503 takes the instance out of rotation; a restart would not help"
    );

    let body = json_body(degraded).await;
    assert_eq!(body["ready"], false);
    assert_eq!(
        body["database"], true,
        "a healthy dependency still reports healthy — the fields are separate \
         so an operator knows which incident they have"
    );
    assert_eq!(body["queue"], false);
}

/// The decision fails closed on either dependency.
#[test]
fn test_ready_decision_needs_every_dependency() {
    assert!(ready_decision(ALL_HEALTHY));
    for inputs in [
        ReadyInputs {
            database: false,
            queue: true,
        },
        ReadyInputs {
            database: true,
            queue: false,
        },
        ReadyInputs {
            database: false,
            queue: false,
        },
    ] {
        assert!(
            !ready_decision(inputs),
            "{inputs:?} must not be ready: health.zig's readyDecision is an AND"
        );
    }
}

/// Exactly the ported routes are mounted; every other tabled one answers 404.
///
/// The route table carries all eighty-one endpoints, and this binary serves
/// five: the two probes, the runner's self read and heartbeat, and the
/// enrolment that mints the credential those two are held by. That gap is the
/// thing a reader is most likely to misread, so it is asserted rather than
/// described — and it fails the day a route is mounted without being listed
/// here, which is the only way an unfinished surface goes live by accident.
#[tokio::test]
async fn test_only_the_ported_routes_are_mounted() {
    for route in Route::all() {
        let template = route.meta().template;
        // Only templates with no path parameters can be requested verbatim;
        // a parameterised one would have to be filled in to be requested at all.
        if template.contains('{') {
            continue;
        }
        let response = send(Method::GET, template, ALL_HEALTHY).await;
        let mounted = matches!(
            route,
            Route::Ops(OpsRoute::Healthz | OpsRoute::Readyz)
                | Route::Runner(RunnerRoute::SelfRecord | RunnerRoute::Heartbeat)
                | Route::RunnerOps(RunnerOpsRoute::Register)
        );

        if mounted {
            // Not a 200: a mounted route answers its guard's refusal, or a 405
            // when it serves a method other than the GET sent here. What is
            // being asserted is that the PATH resolves.
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "{template} is one of the routes this binary serves"
            );
        } else {
            assert_eq!(
                response.status(),
                StatusCode::NOT_FOUND,
                "{template} is tabled but not served by this binary, so it must \
                 answer 404 rather than claim to exist"
            );
        }
    }
}
