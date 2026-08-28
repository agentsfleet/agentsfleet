//! The catalogue read's refusal matrix — everything in FRONT of the verb.
//!
//! # What these tests can and cannot prove
//!
//! The harness catalogue refuses like a datastore outage, so the ETag/304
//! exchange — which needs a BODY to hash — rides the integration lane; the
//! validator arithmetic itself is unit-proven in `afd_api::etag`. What this
//! suite pins is the guard, the scope-free admission, the bounds refusals,
//! and the cursor's two DISTINCT refusals: a token nobody issued against a
//! real token for a different query. The cursors are minted through the same
//! codec production uses, so the suite cannot drift from the wire format.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod harness;

use afd_auth::scope::ScopeSet;
use afd_tenant::models::cursor::{CURSOR_VERSION, Cursor, render};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::Fleet;

/// The catalogue's path.
const MODELS: &str = "/v1/models";

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tcafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2models";

/// No capabilities at all — the route demands NONE, and an empty set is what
/// proves the scope rung admits rather than the fixture smuggling one in.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// A well-formed boundary id for minted cursors.
const BOUNDARY_ID: &str = "0195b4ba-8d3a-7f13-8abc-cd0000000002";

/// A GET at `path` presenting `credential`, against a fresh router.
async fn read(path: &str, credential: Option<&str>) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, NO_SCOPES)
        .router();
    harness::send(&router, Method::GET, path, credential, "").await
}

/// Reads a problem document's `detail` and `error_code` back.
async fn refusal_of(response: axum::response::Response) -> (String, String) {
    let document = harness::json_body(response).await;
    let field = |name: &str| {
        document
            .get(name)
            .and_then(Value::as_str)
            .expect("every refusal carries the envelope's fields")
            .to_owned()
    };
    (field("detail"), field("error_code"))
}

/// A cursor the production codec issued for the default query.
fn minted(limit: u32, provider: Option<&str>) -> String {
    render(&Cursor {
        v: CURSOR_VERSION,
        display_key: "claude-sonnet-5".to_owned(),
        vendor_key: "anthropic".to_owned(),
        id: BOUNDARY_ID.to_owned(),
        provider: provider.map(str::to_owned),
        limit,
    })
}

#[tokio::test]
async fn the_catalogue_needs_a_credential_and_nothing_more() {
    let refused = read(MODELS, None).await;
    assert_eq!(
        refused.status(),
        StatusCode::UNAUTHORIZED,
        "the catalogue prices the billing spine and has no anonymous consumer"
    );

    // The same person with an EMPTY scope set gets past the rung — reaching
    // the verb's 503 is the proof the route demands no capability.
    let admitted = read(MODELS, Some(TENANT_KEY)).await;
    assert_eq!(admitted.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn every_wrong_limit_earns_the_bounds_refusal() {
    for wrong in ["0", "101", "abc", "12.5", "-1"] {
        let path = format!("{MODELS}?limit={wrong}");
        let response = read(&path, Some(TENANT_KEY)).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        let (detail, code) = refusal_of(response).await;
        assert_eq!(
            detail,
            afd_api::handler::tenant::DETAIL_CATALOGUE_LIMIT,
            "{path}"
        );
        assert_eq!(code, "UZ-LIBRARY-003", "{path}");
    }
}

#[tokio::test]
async fn an_empty_limit_is_the_default_page() {
    let path = format!("{MODELS}?limit=");
    let response = read(&path, Some(TENANT_KEY)).await;
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "absent and empty both mean the default, so the verb is reached"
    );
}

#[tokio::test]
async fn an_oversized_provider_filter_is_refused() {
    let path = format!("{MODELS}?provider={}", "p".repeat(129));
    let response = read(&path, Some(TENANT_KEY)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let (detail, code) = refusal_of(response).await;
    assert_eq!(detail, afd_api::handler::tenant::DETAIL_PROVIDER_BOUNDS);
    assert_eq!(code, "UZ-LIBRARY-003");
}

#[tokio::test]
async fn a_whitespace_provider_normalizes_to_absent() {
    // `?provider=%20%20` is the same request as omitting the filter — the
    // Zig normalizer's rule, proven by reaching the verb.
    let path = format!("{MODELS}?provider=%20%20");
    let response = read(&path, Some(TENANT_KEY)).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn a_token_nobody_issued_is_refused_as_malformed() {
    for foreign in ["!!not-base64!!", "aGVsbG8"] {
        let path = format!("{MODELS}?starting_after={foreign}");
        let response = read(&path, Some(TENANT_KEY)).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        let (detail, code) = refusal_of(response).await;
        assert_eq!(
            detail,
            afd_api::handler::tenant::DETAIL_CURSOR_MALFORMED,
            "{path}"
        );
        assert_eq!(code, "UZ-LIBRARY-001", "{path}");
    }
}

#[tokio::test]
async fn a_real_cursor_for_a_different_query_is_refused_distinctly() {
    // Minted by the production codec for limit=50; presented at limit=25.
    // The split between 001 and 002 is the load-bearing assertion: folding
    // them would hide a filter change inside the same signal as a bad URL.
    let path = format!("{MODELS}?limit=25&starting_after={}", minted(50, None));
    let response = read(&path, Some(TENANT_KEY)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let (detail, code) = refusal_of(response).await;
    assert_eq!(detail, afd_api::handler::tenant::DETAIL_CURSOR_MISMATCH);
    assert_eq!(code, "UZ-LIBRARY-002");

    // A provider mismatch is the same second refusal.
    let path = format!("{MODELS}?starting_after={}", minted(50, Some("openai")));
    let response = read(&path, Some(TENANT_KEY)).await;
    let (detail, code) = refusal_of(response).await;
    assert_eq!(detail, afd_api::handler::tenant::DETAIL_CURSOR_MISMATCH);
    assert_eq!(code, "UZ-LIBRARY-002");
}

#[tokio::test]
async fn a_matching_cursor_reaches_the_verb() {
    let path = format!(
        "{MODELS}?provider=anthropic&starting_after={}",
        minted(50, Some("anthropic"))
    );
    let response = read(&path, Some(TENANT_KEY)).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn a_cursor_whose_id_is_no_identifier_is_malformed() {
    // Well-formed JSON, right version, matching query — but the id would
    // ride the page SQL as a `::uuid` cast, so it must fail here as the
    // malformed input it is rather than downstream as a 500.
    let token = render(&Cursor {
        v: CURSOR_VERSION,
        display_key: "d".to_owned(),
        vendor_key: "v".to_owned(),
        id: "not-a-uuid".to_owned(),
        provider: None,
        limit: 50,
    });
    let path = format!("{MODELS}?starting_after={token}");
    let response = read(&path, Some(TENANT_KEY)).await;
    let (detail, code) = refusal_of(response).await;
    assert_eq!(detail, afd_api::handler::tenant::DETAIL_CURSOR_MALFORMED);
    assert_eq!(code, "UZ-LIBRARY-001");
}

#[tokio::test]
async fn an_empty_cursor_is_the_first_page() {
    let path = format!("{MODELS}?starting_after=");
    let response = read(&path, Some(TENANT_KEY)).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
