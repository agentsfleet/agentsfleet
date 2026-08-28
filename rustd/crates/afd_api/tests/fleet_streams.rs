//! The two live-stream routes: who reaches them, and what the ceiling answers.
//!
//! # Reading a response without reading its body
//!
//! A stream that opens never ends — that is what a stream is — so nothing here
//! may await the body. Every case reads the STATUS and the headers, which are
//! written before the first frame and are the whole of what a client branches
//! on when deciding whether it got a stream at all. The frames themselves need
//! a live hub and belong to the integration lane.
//!
//! # The routing case is why this file exists
//!
//! `/events/stream` and `/events/{event_id}` are siblings under one prefix. For
//! as long as the stream route was unmounted, a `GET …/events/stream` matched
//! the ITEM template and answered "Event not found" — a live defect with a
//! registry code, on a path a dashboard opens. The fix is that `matchit` ranks a
//! literal segment above a parameter; the test is here because that ranking is a
//! property of a library, not of this code, and a library's behaviour is exactly
//! what a port should pin rather than assume.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod harness;

use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2streams";

/// A well-formed workspace identifier that is somebody else's.
const FOREIGN_WORKSPACE: &str = "01924f4e-0000-7000-8000-0000000000ff";

/// A well-formed fleet identifier the fixture addresses.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";

/// The rung both stream routes take.
const FLEET_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetRead]);

/// The empty set, proving a refusal is the scope rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// The sentence the ownership layer refuses with.
const DETAIL_NOT_YOURS: &str = "Workspace access denied";

/// One fleet's live tail.
fn fleet_stream() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/events/stream")
}

/// The whole workspace's.
fn workspace_stream() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/events/stream")
}

/// One request at a fresh router carrying at most `streams` at once.
async fn send_at(
    streams: usize,
    scopes: ScopeSet,
    path: &str,
    credential: Option<&str>,
) -> axum::response::Response {
    let router = Fleet::new()
        .carrying_at_most(streams)
        .with_person(TENANT_KEY, SUBJECT, scopes)
        .router();
    harness::send(&router, Method::GET, path, credential, "").await
}

/// One request at a router with room for every stream a case opens.
async fn send(scopes: ScopeSet, path: &str, credential: Option<&str>) -> axum::response::Response {
    send_at(8, scopes, path, credential).await
}

/// Reads a problem document's `error_code` back.
///
/// Safe on a refusal, which has a finite body — never called on a response that
/// opened a stream.
async fn code_of(response: axum::response::Response) -> String {
    let document = harness::json_body(response).await;
    let carried = document.get("error_code").and_then(Value::as_str);
    carried
        .expect("every refusal carries a registry code")
        .to_owned()
}

/// Both templates, as a client addresses them.
fn both_streams() -> [String; 2] {
    [workspace_stream(), fleet_stream()]
}

/// Neither stream is reachable without a credential.
#[tokio::test]
async fn neither_stream_is_anonymous() {
    for path in both_streams() {
        let response = send(FLEET_READ, &path, None).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{path} must refuse an anonymous caller"
        );
    }
}

/// Both streams are refused by the scope rung before anything else.
#[tokio::test]
async fn both_streams_are_refused_by_the_scope_rung() {
    for path in both_streams() {
        let response = send(NO_SCOPES, &path, Some(TENANT_KEY)).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{path} must be refused by the rung"
        );
    }
}

/// A principal streaming somebody else's workspace is refused by the layer.
///
/// The refusal that matters most on this surface: a stream that got past the
/// ownership check would deliver another tenant's activity for as long as the
/// tab stayed open, and nothing in the client would say so.
#[tokio::test]
async fn a_principal_in_a_foreign_workspace_is_refused_by_the_layer() {
    for path in [
        format!("/v1/workspaces/{FOREIGN_WORKSPACE}/events/stream"),
        format!("/v1/workspaces/{FOREIGN_WORKSPACE}/fleets/{FLEET}/events/stream"),
    ] {
        let response = send(FLEET_READ, &path, Some(TENANT_KEY)).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{path} must be refused before a channel is subscribed"
        );
        let document = harness::json_body(response).await;
        assert_eq!(
            document.get("detail").and_then(Value::as_str),
            Some(DETAIL_NOT_YOURS),
            "{path}: the refusal is the ownership layer's"
        );
    }
}

/// `/events/stream` is the STREAM route, not an event whose id is "stream".
///
/// The live defect this file was written for. Both templates live under one
/// prefix and `matchit` ranks the literal above the parameter — so a stream
/// route mounted beside an item route wins. Asserted as "not the item's
/// refusal", because the stream route's own answer is an open connection that
/// this test must not read.
#[tokio::test]
async fn the_stream_path_is_not_read_as_an_event_identifier() {
    let response = send(FLEET_READ, &fleet_stream(), Some(TENANT_KEY)).await;
    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "the literal segment must outrank the event-id parameter"
    );

    // And the item route still serves its own template, so the win is the
    // ranking rather than the parameter route having stopped matching.
    let item = format!(
        "/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/events/01924f4e-0000-7000-8000-000000000abc"
    );
    let detail = send(FLEET_READ, &item, Some(TENANT_KEY)).await;
    assert_eq!(
        detail.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "a real event id still reaches the item route and its unreachable store"
    );
}
/// An authorised stream reaches the store, and the outage is the STORE's.
///
/// The distinguishing assertion at this tier. Both routes read Postgres before
/// a channel is subscribed — the fleet one to prove the fleet is this
/// workspace's, the workspace one to enumerate what to attach — so against the
/// fixture's unreachable pool a well-formed request answers `UZ-INTERNAL-001`
/// and NOT `UZ-API-002`. That is what proves the ceiling admitted it: a request
/// the ceiling refused never reaches a statement and says so with its own code.
///
/// A stream that actually OPENS needs a live Postgres and a live hub, so the
/// `text/event-stream` headers and the frames on them are the integration
/// lane's, not this one's.
#[tokio::test]
async fn an_authorised_stream_reaches_the_store_and_reports_the_outage() {
    for path in both_streams() {
        let response = send(FLEET_READ, &path, Some(TENANT_KEY)).await;
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{path} must reach the store"
        );
        assert_eq!(
            code_of(response).await,
            error_code::INTERNAL_DB_UNAVAILABLE.as_str(),
            "{path}: the refusal is the datastore's, not the ceiling's"
        );
    }
}

/// A stream refused below the ceiling gives its slot back.
///
/// The slot is claimed before any datastore work and released by `Drop` when
/// the handler returns without a stream, so a run of refusals cannot silently
/// exhaust an instance. Proven against a ceiling of ONE: three consecutive
/// attempts are each still refused by the store rather than by the ceiling,
/// which they could not be if the first had kept its slot.
#[tokio::test]
async fn a_refused_stream_gives_its_slot_back() {
    let router = Fleet::new()
        .carrying_at_most(1)
        .with_person(TENANT_KEY, SUBJECT, FLEET_READ)
        .router();

    for attempt in 0..3_u8 {
        let response = harness::send(
            &router,
            Method::GET,
            &workspace_stream(),
            Some(TENANT_KEY),
            "",
        )
        .await;
        assert_eq!(
            code_of(response).await,
            error_code::INTERNAL_DB_UNAVAILABLE.as_str(),
            "attempt {attempt} must still be refused by the store, not the ceiling"
        );
    }
}

/// A ceiling of zero serves no streams at all.
///
/// The knob an operator sets to turn the surface off. It must read as "none"
/// rather than "unlimited" — every other ceiling in this daemon counts up to
/// its value.
#[tokio::test]
async fn an_instance_carrying_no_streams_refuses_every_one() {
    let response = send_at(0, FLEET_READ, &workspace_stream(), Some(TENANT_KEY)).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(code_of(response).await, error_code::SSE_STREAM_CAP.as_str());
}

/// The stream templates carry only GET.
#[tokio::test]
async fn the_stream_templates_carry_only_a_read() {
    for method in [Method::POST, Method::PATCH, Method::DELETE] {
        for path in both_streams() {
            let router = Fleet::new()
                .with_person(TENANT_KEY, SUBJECT, FLEET_READ)
                .router();
            let response =
                harness::send(&router, method.clone(), &path, Some(TENANT_KEY), "").await;
            assert_eq!(
                response.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "{method} {path} is not served"
            );
        }
    }
}
