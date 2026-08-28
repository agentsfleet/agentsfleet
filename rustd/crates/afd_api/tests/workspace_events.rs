//! Who may read the narrative log, and what the PATH has to say before a
//! statement is written.
//!
//! Sibling of `workspace_events_input.rs`, which owns the query string. Split
//! along the axis each proves: this one is the bearer guard, the scope rung,
//! the ownership layer and the three path segments; that one is everything a
//! caller gets wrong once past them. `fleet:read` reaches all three routes
//! alike, and that is pinned per route rather than once: the rung is declared
//! on each route's own table row, so three rows can disagree and only a
//! per-route assertion notices.
//!
//! # What this suite deliberately does not prove
//!
//! No row is ever read. The harness drives the production `History` over a
//! Postgres nobody is listening on, so a request that gets all the way through
//! answers `503` — and that refusal is precisely the evidence the request
//! REACHED the store. Ordering, cursor arithmetic, the `LIKE` predicate, a
//! rendered row and the `404` an absent event earns all need a statement that
//! ran, and live in the `#[ignore]`d lane `make test-integration-rustd` runs.
//!
//! An EMPTY path segment is absent for a different reason: it reaches no
//! handler in either daemon — `route_matchers.zig::Path::param` refuses one at
//! the matcher and axum answers a bare 404 for a trailing one — so the
//! handler's own empty-`event_id` guard has no served behaviour to pin.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod harness;

use std::collections::HashMap;

use afd_api::route::FleetRoute;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code::{self, ErrorCode};
use afd_core::problem::Problem;
use axum::Router;
use axum::extract::Path;
use axum::response::Response;
use axum::routing::get;
use http::{Method, StatusCode};

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_te7e7e7e7decafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbad";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2events";

/// A well-formed workspace identifier that is somebody else's.
const FOREIGN_WORKSPACE: &str = "01924f4e-0000-7000-8000-0000000000ee";

/// A second, so "not yours" and "no such thing" can be compared.
const UNKNOWN_WORKSPACE: &str = "01924f4e-0000-7000-8000-0000000000ed";

/// A well-formed fleet identifier the fixture addresses.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000f1ee";

/// An event identifier as a producer writes one — a Redis stream id, which is
/// exactly why nothing about its shape can be validated.
const EVENT: &str = "1785699668169-0";

/// The longest event identifier the expanded read will look up.
const EVENT_ID_MAX_LEN: usize = 256;

/// The one rung all three routes declare.
const FLEET_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetRead]);

/// The empty set, proving a refusal is the rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// What a caller is told, as the three facts they act on: a status they retry
/// on, a code their client branches on, a sentence a person reads.
type Refusal = (StatusCode, &'static str, ErrorCode);

/// The ownership layer's refusal, byte-for-byte as the Zig handlers spell it.
const NOT_YOURS: Refusal = (
    StatusCode::FORBIDDEN,
    "Workspace access denied",
    error_code::AUTH_FORBIDDEN,
);

/// What a workspace segment that is not an identifier earns.
const BAD_WORKSPACE_ID: Refusal = (
    StatusCode::BAD_REQUEST,
    "workspace_id must be a valid UUIDv7",
    error_code::UUIDV7_INVALID_ID_SHAPE,
);

/// A request this daemon cannot read at all: `UZ-REQ-001` and a 400, whatever
/// the sentence — which every refusal a handler writes about its own path
/// segments is, so the pair is stated once rather than per case.
const fn malformed(detail: &'static str) -> Refusal {
    (StatusCode::BAD_REQUEST, detail, error_code::INVALID_REQUEST)
}

/// What an unreachable datastore earns. Spelled out rather than imported:
/// `afd_events` keeps the sentence private, and a byte-parity suite wants its
/// own copy of the bytes anyway.
const DATASTORE_DOWN: Refusal = (
    StatusCode::SERVICE_UNAVAILABLE,
    "Database unavailable",
    error_code::INTERNAL_DB_UNAVAILABLE,
);

/// The whole workspace's history.
fn workspace_history(workspace: &str) -> String {
    format!("/v1/workspaces/{workspace}/events")
}

/// One fleet's history.
fn fleet_history(workspace: &str, fleet: &str) -> String {
    format!("/v1/workspaces/{workspace}/fleets/{fleet}/events")
}

/// One event, expanded.
fn one_event(workspace: &str, fleet: &str, event: &str) -> String {
    format!("/v1/workspaces/{workspace}/fleets/{fleet}/events/{event}")
}

/// Every route on this surface, under `workspace`.
fn every_route(workspace: &str) -> [String; 3] {
    [
        workspace_history(workspace),
        fleet_history(workspace, FLEET),
        one_event(workspace, FLEET, EVENT),
    ]
}

/// One request at a fresh router holding one scoped person.
async fn send(scopes: ScopeSet, method: Method, path: &str, credential: Option<&str>) -> Response {
    let fleet = Fleet::new().with_person(TENANT_KEY, SUBJECT, scopes);
    harness::send(&fleet.router(), method, path, credential, "").await
}

/// One fully authorised read, so what answers is the axis under test.
async fn authorised(path: &str) -> Response {
    send(FLEET_READ, Method::GET, path, Some(TENANT_KEY)).await
}

/// A response reduced to the three facts a [`Refusal`] names.
async fn refusal_of(response: Response) -> (StatusCode, String, String) {
    let status = response.status();
    let body = harness::json_body(response).await;
    let read = |key: &str| body[key].as_str().expect("a refusal carries it").to_owned();
    (status, read("detail"), read("error_code"))
}

/// Asserts a response is exactly the refusal named.
async fn assert_refusal(response: Response, expected: Refusal, case: &str) {
    let (status, detail, code) = refusal_of(response).await;
    let seen = (status, detail.as_str(), code.as_str());
    assert_eq!(
        seen,
        (expected.0, expected.1, expected.2.as_str()),
        "{case}"
    );
}

/// No route here is reachable without a credential.
#[tokio::test]
async fn no_route_on_this_surface_is_anonymous() {
    for path in every_route(OWNED_WORKSPACE) {
        let answer = send(FLEET_READ, Method::GET, &path, None).await;
        assert_eq!(answer.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}

/// Every route is refused by the scope rung before anything else.
#[tokio::test]
async fn every_route_is_refused_by_the_scope_rung_before_anything_else() {
    for path in every_route(OWNED_WORKSPACE) {
        let refused = send(NO_SCOPES, Method::GET, &path, Some(TENANT_KEY)).await;
        let (status, _sentence, code) = refusal_of(refused).await;
        // A missing capability is not a missing workspace, and a client that
        // conflated them would tell a person to ask for the wrong thing.
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(code, error_code::AUTH_INSUFFICIENT_SCOPE.as_str(), "{path}");
    }
}

/// A principal acting in somebody else's workspace runs no statement.
#[tokio::test]
async fn a_principal_in_a_foreign_workspace_is_refused_by_the_layer() {
    for path in every_route(FOREIGN_WORKSPACE) {
        assert_refusal(authorised(&path).await, NOT_YOURS, &path).await;
    }
}

/// A workspace that does not exist answers exactly as one that is not yours.
///
/// The collapse is the LAYER's and not the fixture's: `own` renders one refusal
/// for `Ok(None)`, and a resolver over real Postgres answers `None` for both a
/// foreign workspace and an identifier nobody minted. Were the two
/// distinguishable, this surface would be an oracle for which workspace
/// identifiers are real, probeable without holding any of them.
#[tokio::test]
async fn a_workspace_that_does_not_exist_is_not_distinguishable_from_a_foreign_one() {
    let foreign = authorised(&workspace_history(FOREIGN_WORKSPACE)).await;
    let unknown = authorised(&workspace_history(UNKNOWN_WORKSPACE)).await;

    assert_eq!(refusal_of(foreign).await, refusal_of(unknown).await);
}

/// A workspace segment that is not an identifier never reaches a handler.
#[tokio::test]
async fn a_workspace_segment_that_is_not_an_identifier_never_reaches_a_handler() {
    for path in every_route("not-a-uuid") {
        assert_refusal(authorised(&path).await, BAD_WORKSPACE_ID, &path).await;
    }
}

/// A fleet segment that is not an identifier is refused before the store.
///
/// Both fleet-addressed routes, same sentence on each: the `::uuid` cast in the
/// statement must never be the thing that fails, so every error from below
/// stays a genuine datastore fault. The version nibble is one of the shapes
/// because a v4 identifier is what a caller most likely has on hand, and a
/// check that only counted hyphens would take it.
#[tokio::test]
async fn a_fleet_segment_that_is_not_a_uuidv7_is_refused_before_the_store() {
    for shape in ["not-a-uuid", "01924f4e-0000-4000-8000-00000000f1ee"] {
        let paths = [
            fleet_history(OWNED_WORKSPACE, shape),
            one_event(OWNED_WORKSPACE, shape, EVENT),
        ];
        for path in paths {
            let expected = malformed("fleet_id must be a UUIDv7");
            assert_refusal(authorised(&path).await, expected, &path).await;
        }
    }
}

/// Once every segment is valid, all three routes reach the event store.
///
/// The positive half of the matrix, and what proves `fleet:read` alone is the
/// whole rung: a route still refused here is refused by something this suite
/// has not named. A `503` and not merely "not a 4xx", because the transport
/// class is what the runner client's backoff reads (RULE ECL).
#[tokio::test]
async fn every_route_reaches_the_store_once_its_path_is_valid() {
    for path in every_route(OWNED_WORKSPACE) {
        assert_refusal(authorised(&path).await, DATASTORE_DOWN, &path).await;
    }
}

/// An event identifier too long to look up is refused, and one at the bound
/// is not.
///
/// `event_id` is TEXT written by the producer, so there is no shape to check —
/// the bound only refuses an identifier long enough to be an attack on the
/// index rather than a lookup. Both sides in one case, because the off-by-one a
/// `>=` in place of a `>` would introduce is invisible from either half alone:
/// it would refuse a legitimate identifier and read, to whoever held it, as an
/// event that had gone missing.
#[tokio::test]
async fn an_event_id_is_looked_up_up_to_the_length_bound_and_not_past_it() {
    let at_bound = one_event(OWNED_WORKSPACE, FLEET, &"e".repeat(EVENT_ID_MAX_LEN));
    assert_refusal(authorised(&at_bound).await, DATASTORE_DOWN, "256 bytes").await;

    let past_it = one_event(OWNED_WORKSPACE, FLEET, &"e".repeat(EVENT_ID_MAX_LEN + 1));
    let expected = malformed("event_id is required");
    assert_refusal(authorised(&past_it).await, expected, "257 bytes").await;
}

/// The code an absent event answers with is the event's own, not the fleet's.
///
/// Against the registry rather than over HTTP, and the harness is the reason:
/// reaching `UZ-AGT-015` needs a statement that RAN and found nothing, which is
/// a live-Postgres fact. What holds with no datastore is that the code exists,
/// is a 404, and is not the fleet's — an operator reading it should not have to
/// guess which of the two was missing. `event_detail.zig` asserts this pair for
/// exactly that reason.
#[test]
fn the_absent_event_code_is_the_events_own_and_not_the_fleets() {
    assert_ne!(
        error_code::EVENT_NOT_FOUND.as_str(),
        error_code::AGENTSFLEET_NOT_FOUND.as_str()
    );
    assert_eq!(Problem::of(error_code::EVENT_NOT_FOUND).status(), 404);
}

/// The templates carry only the methods they document.
#[tokio::test]
async fn the_templates_carry_only_the_methods_they_document() {
    // History is written by a run, never by a person: the row IS the audit
    // trail of what a fleet did, so nothing here writes, edits or deletes one.
    let refused = [
        (Method::POST, workspace_history(OWNED_WORKSPACE)),
        (Method::POST, fleet_history(OWNED_WORKSPACE, FLEET)),
        (Method::DELETE, one_event(OWNED_WORKSPACE, FLEET, EVENT)),
        (Method::PUT, one_event(OWNED_WORKSPACE, FLEET, EVENT)),
    ];
    for (method, path) in refused {
        let answer = send(FLEET_READ, method.clone(), &path, Some(TENANT_KEY)).await;
        assert_eq!(
            answer.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {path}"
        );
    }
}

/// The expanded read's template does not shadow the live tail's.
///
/// `/events/stream` and `/events/{event_id}` are the same depth, and `event_id`
/// is free-form TEXT — nothing about its shape excludes the word `stream`. What
/// keeps them apart is that the router resolves a static segment ahead of a
/// parameter, and what is pinned is that the two TEMPLATES the route table
/// declares are still resolvable that way.
///
/// Driven over a router built from those templates rather than the served one,
/// for the reason `event_detail.zig` gives for asserting against `router.match`
/// instead of over HTTP: the live tail never closes its connection, so asking
/// the real route a question hangs the suite. Reading the templates off
/// [`FleetRoute`] is what makes this fail when somebody edits either row.
#[tokio::test]
async fn the_expanded_reads_template_does_not_shadow_the_live_tail() {
    async fn tail() -> StatusCode {
        StatusCode::IM_A_TEAPOT
    }
    async fn expanded(Path(bound): Path<HashMap<String, String>>) -> String {
        bound.get("event_id").cloned().unwrap_or_default()
    }
    let router = Router::new()
        .route(FleetRoute::EventsStream.meta().template, get(tail))
        .route(FleetRoute::Event.meta().template, get(expanded));
    let ask = async |event: &str| {
        let path = one_event(OWNED_WORKSPACE, FLEET, event);
        harness::send(&router, Method::GET, &path, None, "").await
    };

    assert_eq!(
        ask("stream").await.status(),
        StatusCode::IM_A_TEAPOT,
        "the live tail keeps its own path"
    );

    // And the parameter binds the WHOLE segment, separator and all.
    let read = ask(EVENT).await;
    assert_eq!(read.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(read.into_body(), usize::MAX)
        .await
        .expect("the fixture body is small and in memory");
    assert_eq!(String::from_utf8_lossy(&bytes), EVENT);
}
