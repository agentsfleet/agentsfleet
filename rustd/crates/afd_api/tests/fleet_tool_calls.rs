//! The single-call read's guard, its rung, and the refusals it decides before
//! any datastore is reached.
//!
//! Row behaviour — a member reading a kept record, another workspace's fleet
//! answering 404 — needs live Postgres and is proven in the integration lane
//! (`agentsfleetd/tests/integration_tool_call_details.rs`).
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2toolcalls";

/// A well-formed fleet identifier the fixture addresses.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";

/// Reading a fleet.
const FLEET_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetRead]);

/// The empty set, proving a refusal is the scope rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// The read for `call_id` on a fixture event of `fleet`.
fn tool_call(fleet: &str, call_id: &str) -> String {
    format!(
        "/v1/workspaces/{OWNED_WORKSPACE}/fleets/{fleet}/events/1700000000000-0/tool-calls/{call_id}"
    )
}

/// One GET at a fresh router holding one scoped person.
async fn read(scopes: ScopeSet, path: &str, credential: Option<&str>) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, scopes)
        .router();
    harness::send(&router, Method::GET, path, credential, "").await
}

/// The registry code a refusal carries.
async fn code_of(response: axum::response::Response) -> String {
    harness::error_code(&harness::json_body(response).await)
        .expect("every refusal carries a code")
        .to_owned()
}

#[tokio::test]
async fn test_tool_call_detail_requires_fleet_read() {
    let path = tool_call(FLEET, "7:3");
    assert_eq!(
        read(FLEET_READ, &path, None).await.status(),
        StatusCode::UNAUTHORIZED,
        "never anonymous"
    );
    assert_eq!(
        read(NO_SCOPES, &path, Some(TENANT_KEY)).await.status(),
        StatusCode::FORBIDDEN,
        "the rung refuses a caller without fleet:read"
    );
}

#[tokio::test]
async fn a_call_id_that_is_not_one_answers_as_an_unknown_call() {
    for malformed in ["x:y:z", "7", "7:0"] {
        let refused = read(FLEET_READ, &tool_call(FLEET, malformed), Some(TENANT_KEY)).await;
        assert_eq!(refused.status(), StatusCode::NOT_FOUND, "{malformed}");
        assert_eq!(
            code_of(refused).await,
            afd_core::error_code::TOOL_CALL_NOT_FOUND.as_str()
        );
    }
}

#[tokio::test]
async fn a_fleet_or_event_this_surface_will_not_look_up_is_malformed() {
    let refused = read(
        FLEET_READ,
        &tool_call("not-a-uuid", "7:3"),
        Some(TENANT_KEY),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);

    let long_event = format!(
        "/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/events/{}/tool-calls/7:3",
        "e".repeat(257)
    );
    let refused = read(FLEET_READ, &long_event, Some(TENANT_KEY)).await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_tool_call_routes_are_mounted() {
    // Past the guard and the parse, the read reaches its store; with no
    // datastore behind the harness that is a 503, never the router's 404.
    let reached = read(FLEET_READ, &tool_call(FLEET, "7:3"), Some(TENANT_KEY)).await;
    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
}
