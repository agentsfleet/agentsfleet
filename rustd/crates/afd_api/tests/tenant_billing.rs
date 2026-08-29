//! The billing reads' refusal matrix — everything in FRONT of the verb.
//!
//! # Why this suite is about the refusals and not the money
//!
//! Both verbs are one statement a real Postgres evaluates, and the one rule
//! only a seeded database can show — a missing wallet row refusing as a
//! bootstrap-invariant violation rather than a 404 — needs rows to be missing
//! FROM. So the harness answers the refusal a datastore that would not answer
//! gives, and what these tests pin is the guard, the scope rung, the
//! query-string refusals and the cursor boundary: everything a request meets
//! before the pool is asked.
//!
//! # The sentences are the assertion
//!
//! The limit refusals and the tenant sentence are the Zig handler's bytes, and
//! a dashboard mid-cutover may be matching on them. Each is asserted against
//! the constant the handler answers with rather than a respelling here
//! (RULE UFS).
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::Fleet;

/// The wallet snapshot's path.
const BILLING: &str = "/v1/tenants/me/billing";

/// The charges walk's path.
const CHARGES: &str = "/v1/tenants/me/billing/charges";

/// A tenant api-key, shaped as the authenticator classifies one — the marker
/// plus sixty-four lower-case hex.
const TENANT_KEY: &str = "agt_tfeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedface";

/// An `afc_` credential — the same hex under the terminal's marker, so the
/// only axis between the two fixtures is credential class.
const TERMINAL: &str = "afc_feedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedface";

/// The subject the fixture credentials resolve to.
const SUBJECT: &str = "user_2billing";

/// What the route table demands of both reads.
const BILLING_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::BillingRead]);

/// The empty set, proving a refusal below is the scope rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// The sentence the scope rung refuses with, as `Denied` renders it.
const DETAIL_SCOPE: &str = "Requires scope billing:read";

/// A GET at `path` presenting `credential`, against a fresh router.
async fn read(scopes: ScopeSet, path: &str, credential: Option<&str>) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, scopes)
        .router();
    harness::send(&router, Method::GET, path, credential, "").await
}

/// Reads a problem document's `detail` back.
async fn detail_of(response: axum::response::Response) -> String {
    let document = harness::json_body(response).await;
    document
        .get("detail")
        .and_then(Value::as_str)
        .expect("every refusal carries a detail")
        .to_owned()
}

/// Asserts the request got PAST every layer in front of the verb.
///
/// 503 with the datastore sentence is the one answer only the SERVICE can
/// produce: the guard, the scope rung and the query-string parsing all refuse
/// with 4xx codes of their own, so this is what "reached the handler" looks
/// like over a pool that answers nothing.
async fn assert_reached_the_verb(response: axum::response::Response, case: &str) {
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "{case}: only the verb answers with the datastore's refusal"
    );
    assert_eq!(
        detail_of(response).await,
        afd_tenant::error::detail::DETAIL_DATABASE_UNAVAILABLE,
        "{case}: the refusal is the plane's, not a layer's"
    );
}

#[tokio::test]
async fn both_reads_need_a_credential() {
    for path in [BILLING, CHARGES] {
        let response = read(BILLING_READ, path, None).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{path} is not an anonymous read"
        );
    }
}

#[tokio::test]
async fn both_reads_need_the_billing_scope() {
    for path in [BILLING, CHARGES] {
        let response = read(NO_SCOPES, path, Some(TENANT_KEY)).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{path} without billing:read is refused"
        );
        assert_eq!(
            detail_of(response).await,
            DETAIL_SCOPE,
            "{path}: the whole requirement is named, as the Zig daemon words it"
        );
    }
}

#[tokio::test]
async fn a_scoped_person_reaches_both_verbs() {
    for path in [BILLING, CHARGES] {
        let response = read(BILLING_READ, path, Some(TENANT_KEY)).await;
        assert_reached_the_verb(response, path).await;
    }
}

/// The terminal's class is admitted — `agentsfleet billing show` is a thing.
#[tokio::test]
async fn a_command_line_credential_reads_billing_too() {
    let router = Fleet::new()
        .with_terminal(TERMINAL, SUBJECT, BILLING_READ)
        .router();
    let response = harness::send(&router, Method::GET, BILLING, Some(TERMINAL), "").await;
    assert_reached_the_verb(response, "terminal snapshot").await;
}

#[tokio::test]
async fn a_limit_that_is_not_a_number_is_refused() {
    for wrong in ["lots", "-1", "1.5"] {
        let path = format!("{CHARGES}?limit={wrong}");
        let response = read(BILLING_READ, &path, Some(TENANT_KEY)).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(
            detail_of(response).await,
            afd_api::handler::tenant::DETAIL_LIMIT_NOT_NUMERIC,
            "a sign or a fraction is not a count on either daemon"
        );
    }
}

#[tokio::test]
async fn a_limit_outside_the_range_is_refused() {
    for wrong in ["0", "201"] {
        let path = format!("{CHARGES}?limit={wrong}");
        let response = read(BILLING_READ, &path, Some(TENANT_KEY)).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(
            detail_of(response).await,
            afd_api::handler::tenant::DETAIL_LIMIT_RANGE,
            "the range sentence names the same bounds the store enforces"
        );
    }
}

#[tokio::test]
async fn the_limit_cap_is_inside_the_range() {
    let path = format!("{CHARGES}?limit=200");
    let response = read(BILLING_READ, &path, Some(TENANT_KEY)).await;
    assert_reached_the_verb(response, "limit=200").await;
}

#[tokio::test]
async fn a_cursor_this_daemon_never_issued_is_refused() {
    let path = format!("{CHARGES}?cursor=!!not-a-token");
    let response = read(BILLING_READ, &path, Some(TENANT_KEY)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
    assert_eq!(
        detail_of(response).await,
        afd_tenant::error::detail::DETAIL_CHARGES_CURSOR_INVALID,
        "one undifferentiated refusal, because a cursor is opaque"
    );
}

/// `?cursor=` with nothing after it is the first page, not a malformed token.
#[tokio::test]
async fn an_empty_cursor_is_the_first_page() {
    let path = format!("{CHARGES}?cursor=");
    let response = read(BILLING_READ, &path, Some(TENANT_KEY)).await;
    assert_reached_the_verb(response, "empty cursor").await;
}

/// A token the ZIG daemon issued parses at this boundary — the mid-cutover
/// claim, made where a client would actually present it.
#[tokio::test]
async fn a_zig_issued_cursor_is_accepted() {
    let path = format!("{CHARGES}?cursor=MTcxMjkyNDQwMDAwMDphYmMxMjM");
    let response = read(BILLING_READ, &path, Some(TENANT_KEY)).await;
    assert_reached_the_verb(response, "zig cursor").await;
}
