//! Every list route's `?limit`, at both edges of its own ceiling.
//!
//! Each route reads its page size through one bounded type with the route's
//! ceiling and answers the route's own sentence; this suite is the one place
//! that walks all of them, so a route that drifts from the shared reading is
//! caught by name. The sentences are spelled out rather than imported, so the
//! test and the code under test cannot agree with each other by construction.
//!
//! A limit AT the ceiling is accepted, and over a datastore nobody is
//! listening on the route answers past the parameter check — a `503` or a
//! `404` from the store, never the `400` this suite pins for the two edges.
#![cfg(feature = "test-util")]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tc0dec0dec0dec0dec0dec0dec0dec0dec0dec0dec0dec0dec0dec0dec0dec0de";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2list_limits";

/// A well-formed fleet identifier the fixture addresses.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000c0de";

/// A well-formed runner identifier the operator lists address.
const RUNNER: &str = "019329c5-0000-7000-8000-0000000000a1";

/// Every rung a list route below declares, so nothing is refused by a rung.
const EVERY_LIST_SCOPE: ScopeSet = ScopeSet::from_scopes(&[
    Scope::FleetRead,
    Scope::ApprovalRead,
    Scope::BillingRead,
    Scope::WorkspaceAdmin,
    Scope::ApikeyRead,
    Scope::SecretRead,
    Scope::RunnerRead,
]);

/// The malformed-request code most lists answer.
const INVALID_REQUEST: &str = "UZ-REQ-001";

/// The library family's bounds code.
const LIBRARY_BOUNDS: &str = "UZ-LIBRARY-003";

/// The keyset lists' sentence, shared by the library family.
const LIBRARY_LIMIT: &str = "limit must be an integer between 1 and 100";

/// The event listings' and the inbox's sentence.
const WIDE_LIMIT: &str = "limit must be between 1 and 200";

/// One list route: where it lives, its ceiling, and how it refuses.
struct ListRoute {
    path: String,
    ceiling: u32,
    code: &'static str,
    sentence: &'static str,
}

impl ListRoute {
    fn new(path: String, ceiling: u32, code: &'static str, sentence: &'static str) -> Self {
        Self {
            path,
            ceiling,
            code,
            sentence,
        }
    }

    fn with_limit(&self, limit: u32) -> String {
        format!("{}?limit={limit}", self.path)
    }
}

/// Every list route that pages with `?limit`, each with its own ceiling.
fn every_list_route() -> Vec<ListRoute> {
    let workspace = format!("/v1/workspaces/{OWNED_WORKSPACE}");
    vec![
        ListRoute::new(
            format!("{workspace}/fleets/{FLEET}/messages"),
            25,
            INVALID_REQUEST,
            "limit must be between 1 and 25",
        ),
        ListRoute::new(
            format!("{workspace}/events"),
            200,
            INVALID_REQUEST,
            WIDE_LIMIT,
        ),
        ListRoute::new(
            format!("{workspace}/fleets/{FLEET}/events"),
            200,
            INVALID_REQUEST,
            WIDE_LIMIT,
        ),
        ListRoute::new(
            format!("{workspace}/approvals"),
            200,
            INVALID_REQUEST,
            WIDE_LIMIT,
        ),
        ListRoute::new("/v1/models".to_owned(), 100, LIBRARY_BOUNDS, LIBRARY_LIMIT),
        ListRoute::new(
            format!("{workspace}/fleet-libraries"),
            100,
            LIBRARY_BOUNDS,
            LIBRARY_LIMIT,
        ),
        ListRoute::new(
            format!("{workspace}/library-entries"),
            100,
            LIBRARY_BOUNDS,
            LIBRARY_LIMIT,
        ),
        ListRoute::new(
            "/v1/tenants/me/models".to_owned(),
            100,
            LIBRARY_BOUNDS,
            LIBRARY_LIMIT,
        ),
        ListRoute::new(
            "/v1/tenants/me/billing/charges".to_owned(),
            200,
            INVALID_REQUEST,
            WIDE_LIMIT,
        ),
        ListRoute::new(
            "/v1/tenants/me/workspaces".to_owned(),
            100,
            INVALID_REQUEST,
            "Limit must be between 1 and 100",
        ),
        ListRoute::new(
            "/v1/api-keys".to_owned(),
            100,
            INVALID_REQUEST,
            "limit must be between 1 and 100",
        ),
        ListRoute::new(
            "/v1/fleets/runners".to_owned(),
            100,
            INVALID_REQUEST,
            "limit must be an integer between 1 and 100; starting_after must be a cursor from a previous page",
        ),
        ListRoute::new(
            format!("/v1/fleets/runners/{RUNNER}/events"),
            100,
            INVALID_REQUEST,
            "limit must be between 1 and 100; starting_after must be a cursor from a previous page; event_type must be a comma-separated set of runner event types; since/until must be millis",
        ),
        ListRoute::new(
            format!("/v1/fleets/runners/{RUNNER}/leases"),
            100,
            INVALID_REQUEST,
            LIBRARY_LIMIT,
        ),
    ]
}

/// One fully authorised read, so what answers is the parameter under test.
async fn read(path: &str) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, EVERY_LIST_SCOPE)
        .router();
    harness::send(&router, Method::GET, path, Some(TENANT_KEY), "").await
}

/// The status, code and sentence a response carries.
async fn answer(path: &str) -> (StatusCode, Option<String>, Option<String>) {
    let response = read(path).await;
    let status = response.status();
    let document = harness::json_body(response).await;
    let field = |key: &str| document.get(key).and_then(Value::as_str).map(str::to_owned);
    (status, field("error_code"), field("detail"))
}

#[tokio::test]
async fn test_every_list_route_refuses_its_limit_out_of_range() {
    for route in every_list_route() {
        for refused in [0, route.ceiling + 1] {
            let path = route.with_limit(refused);
            let expected = (
                StatusCode::BAD_REQUEST,
                Some(route.code.to_owned()),
                Some(route.sentence.to_owned()),
            );
            assert_eq!(answer(&path).await, expected, "{path}");
        }
        let at_ceiling = route.with_limit(route.ceiling);
        let (status, _code, _detail) = answer(&at_ceiling).await;
        assert_ne!(
            status,
            StatusCode::BAD_REQUEST,
            "{at_ceiling} is inside its ceiling"
        );
        assert_ne!(
            status,
            StatusCode::FORBIDDEN,
            "{at_ceiling} passed its rung"
        );
    }
}
