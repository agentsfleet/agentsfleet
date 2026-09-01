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

use crate::harness;

use afd_api::route::{AuthRoute, OpsRoute, Route, RunnerRoute, TenantRoute};
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

/// This binary now serves every route the table declares.
///
/// The test that stood here asserted the opposite half — that a HEAD at a
/// TABLED BUT UNSERVED path answers 404 rather than the 405 a served path with
/// the wrong method gets. Its own note said each milestone landing a handler
/// group would push its example down the table "until nothing qualifies and the
/// test retires". The workspace fleet-library pair was the last example, and
/// nothing qualifies now.
///
/// What replaces it is the stronger claim the retirement makes available: the
/// route table and the mount are the same set. A route that lost its handler
/// would fail here rather than becoming a silent 404 nobody asserted on.
#[tokio::test]
async fn test_every_tabled_route_is_served() {
    let unserved: Vec<&str> = Route::all()
        .filter(|route| !is_mounted(*route))
        .map(|route| route.meta().template)
        .collect();

    assert!(
        unserved.is_empty(),
        "tabled routes with no handler: {unserved:?}"
    );
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
/// The route table carries all eighty-one endpoint identities, and this binary
/// serves twenty-six: two probes, ten runner-self routes, all seven operator
/// runner routes, all six platform-administration routes, and the public Fleet
/// Bundle gallery. That gap is the thing a reader is most likely to misread, so
/// it is asserted rather than described — and it fails the day a route is
/// mounted without being listed here, which is the only way an unfinished
/// surface goes live by accident.
///
/// Renew, activity, both memory verbs and the bundle fetch are in the matcher
/// below but are never REQUESTED by this loop: their templates carry a path
/// parameter, and the skip above passes over every parameterised path.
///
/// The two memory verbs also share ONE template, differing only by method, so
/// the mount loop merges them into a single `MethodRouter` — axum takes one per
/// path and panics on a second. That merge is exercised by this suite building
/// the router at all: a regression there is a boot panic, not an assertion. Listing it anyway is deliberate — the matcher is the
/// statement of what this binary serves, and leaving a served route out of it
/// because the loop cannot reach it would make the two disagree the moment the
/// skip is lifted.
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
        let mounted = is_mounted(route);

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

/// Whether this binary serves `route`, as a statement independent of the loop.
const fn is_mounted(route: Route) -> bool {
    matches!(
        route,
        Route::Ops(OpsRoute::Healthz | OpsRoute::Readyz)
            | Route::Runner(
                RunnerRoute::SelfRecord
                    | RunnerRoute::Heartbeat
                    | RunnerRoute::Lease
                    | RunnerRoute::Report
                    | RunnerRoute::Renew
                    | RunnerRoute::Activity
                    | RunnerRoute::MemoryHydrate
                    | RunnerRoute::MemoryCapture
                    | RunnerRoute::Bundle
                    | RunnerRoute::CredentialsMint
            )
            | Route::RunnerOps(_)
            | Route::Admin(_)
            // The device-flow login surface, plus the identity-provider
            // delivery beside it. That one is proven by a Svix signature
            // rather than by a bearer, so it mounts through the Auth family's
            // tenant-then-ingress fallthrough — the same shape the connector
            // family already used. It answers 405 to the GET this loop sends,
            // which is a served path refusing a method, not an absent one.
            | Route::Auth(
                AuthRoute::CreateSession
                    | AuthRoute::PollSession
                    | AuthRoute::ApproveSession
                    | AuthRoute::VerifySession
                    | AuthRoute::DeleteSession
                    | AuthRoute::DeleteAllSessions
                    | AuthRoute::IdentityEventClerk
            )
            // M180 §2 and §3's signed ingress, and §4's connector family —
            // both mounts are total, so no verb in either is unserved. Only
            // the QStash fire is reachable by this loop; the rest name a fleet
            // or a provider in their templates and are skipped above. Listed
            // as families anyway, for the reason the note above gives: a
            // served route left out because the loop cannot reach it makes the
            // matcher and the router disagree the moment the skip is lifted.
            | Route::Webhook(_)
            | Route::Connector(_)
            // Every workspace and per-fleet route. The loop above reaches
            // neither, because each template names a workspace or a fleet —
            // which is exactly why both were missing from this matcher until
            // the test below started grading it against the whole table. Listed
            // for the reason that note gives: a served route left out makes the
            // matcher and the router disagree the moment the skip is lifted.
            | Route::Workspace(_)
            | Route::Fleet(_)
            | Route::Tenant(
                TenantRoute::ApiKeys
                    | TenantRoute::ApiKey
                    | TenantRoute::CliCredentials
                    | TenantRoute::CliCredential
                    | TenantRoute::Billing
                    | TenantRoute::BillingCharges
                    | TenantRoute::Workspaces
                    | TenantRoute::CreateWorkspace
                    | TenantRoute::ModelLibrary
                    | TenantRoute::FleetBundles
                    | TenantRoute::Provider
                    | TenantRoute::ModelEntries
                    | TenantRoute::ModelEntry
            )
    )
}
