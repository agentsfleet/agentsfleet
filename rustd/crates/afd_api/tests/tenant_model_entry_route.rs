//! The model registry's refusal matrix — everything in FRONT of the four verbs.
//!
//! # Why the datastore's refusal is the success signal
//!
//! Every verb opens with a read a real Postgres would evaluate, so over the
//! harness's unreachable pool "reached the verb" renders as the 503 only the
//! SERVICE can produce. The guard, the scope rung, the method router, the body
//! parse, the id parse and the cursor check all refuse with codes of their own
//! before any pool is touched — which is exactly what makes each of them
//! provable here, with no datastore in the picture.
//!
//! What is NOT provable this way is anything decided from a row: a duplicate
//! pair, an id that resolves to nothing, an entry that is the active selection.
//! Those are the store's outcomes and they are graded against live Postgres.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use base64::Engine as _;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::Fleet;

/// The collection template.
const ENTRIES: &str = "/v1/tenants/me/models";

/// The item template, with an identifier the parse accepts.
const ENTRY: &str = "/v1/tenants/me/models/0195b4ba-8d3a-7f13-8abc-cd0000000002";

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tfeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedface";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2registry";

/// What the route table demands of the list.
const ENTRIES_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::SecretRead]);

/// What it demands of the writes.
const ENTRIES_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::SecretWrite]);

/// The empty set, proving a refusal below is the scope rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// A body that names both fields the create requires.
const WELL_FORMED_CREATE: &str = r#"{"model_id":"claude-opus-5","secret_ref":"anthropic-prod"}"#;

/// A body that names the one field the change requires.
const WELL_FORMED_UPDATE: &str = r#"{"model_id":"claude-opus-5"}"#;

/// A request at `path`, against a fresh router.
async fn send(
    scopes: ScopeSet,
    method: Method,
    path: &str,
    credential: Option<&str>,
    body: &str,
) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, scopes)
        .router();
    harness::send(&router, method, path, credential, body).await
}

/// Reads a problem document's registry code back.
async fn code_of(response: axum::response::Response) -> String {
    let document = harness::json_body(response).await;
    document
        .get("error_code")
        .and_then(Value::as_str)
        .expect("every refusal names its registry code")
        .to_owned()
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

/// A cursor token for this page, as the handler mints one.
///
/// Spelled here rather than imported: the payload's field ORDER is the wire
/// contract, and a test that built it through the handler's own type would pass
/// even if that order changed under it.
fn token(tenant: &str, limit: u32) -> String {
    let json = format!(
        "{{\"v\":2,\"created_at\":1744000000000,\
\"id\":\"0195b4ba-8d3a-7f13-8abc-cd0000000002\",\
\"tenant_uuid\":\"{tenant}\",\"limit\":{limit}}}"
    );
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
}

#[tokio::test]
async fn test_every_verb_sits_behind_the_bearer_guard() {
    for (method, path) in [
        (Method::GET, ENTRIES),
        (Method::POST, ENTRIES),
        (Method::PATCH, ENTRY),
        (Method::DELETE, ENTRY),
    ] {
        let anonymous = send(ENTRIES_READ, method.clone(), path, None, "").await;
        assert_eq!(
            anonymous.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} with no credential is the guard's refusal, not a 404"
        );
    }
}

#[tokio::test]
async fn test_the_scope_rung_separates_the_list_from_the_writes() {
    // A read-scoped credential may list and may not write; an unscoped one may
    // do neither. The rung answers, not the handler — the pool behind the
    // harness cannot answer anything.
    for (method, path) in [
        (Method::POST, ENTRIES),
        (Method::PATCH, ENTRY),
        (Method::DELETE, ENTRY),
    ] {
        let scoped_read = send(ENTRIES_READ, method.clone(), path, Some(TENANT_KEY), "").await;
        assert_eq!(
            scoped_read.status(),
            StatusCode::FORBIDDEN,
            "{method} {path}: secret:read alone must not authorize a write"
        );
    }

    let unscoped = send(NO_SCOPES, Method::GET, ENTRIES, Some(TENANT_KEY), "").await;
    assert_eq!(
        unscoped.status(),
        StatusCode::FORBIDDEN,
        "no scopes, no list"
    );
}

#[tokio::test]
async fn test_every_verb_reaches_its_service_over_the_dead_pool() {
    // 503 with the datastore sentence is what "past every refusal layer"
    // renders as over a pool that answers nothing — see the module note.
    for (method, path, scopes, body) in [
        (Method::GET, ENTRIES, ENTRIES_READ, ""),
        (Method::POST, ENTRIES, ENTRIES_WRITE, WELL_FORMED_CREATE),
        (Method::PATCH, ENTRY, ENTRIES_WRITE, WELL_FORMED_UPDATE),
        (Method::DELETE, ENTRY, ENTRIES_WRITE, ""),
    ] {
        let reached = send(scopes, method.clone(), path, Some(TENANT_KEY), body).await;
        assert_eq!(
            reached.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{method} {path}: only the verb answers with the datastore's refusal"
        );
        assert_eq!(
            detail_of(reached).await,
            "Database unavailable",
            "{method} {path}: the sentence is the credential plane's outage detail"
        );
    }
}

#[tokio::test]
async fn test_a_body_this_daemon_cannot_read_never_reaches_a_pool() {
    for (method, path) in [(Method::POST, ENTRIES), (Method::PATCH, ENTRY)] {
        let refused = send(ENTRIES_WRITE, method.clone(), path, Some(TENANT_KEY), "{").await;
        assert_eq!(
            refused.status(),
            StatusCode::BAD_REQUEST,
            "{method} {path}: a truncated body is refused before the store"
        );
    }
}

#[tokio::test]
async fn test_the_model_bound_holds_on_both_verbs_that_take_one() {
    // One rule and two call sites, which is exactly how `model_id` ended up
    // bounded on the catalogue route and unbounded on this one. A blank name
    // and an oversized one earn different sentences because the repairs differ.
    const BLANK: &str = r#"{"model_id":"","secret_ref":"anthropic-prod"}"#;
    let oversized = "m".repeat(257);
    for (method, path) in [(Method::POST, ENTRIES), (Method::PATCH, ENTRY)] {
        let refused = send(ENTRIES_WRITE, method.clone(), path, Some(TENANT_KEY), BLANK).await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            detail_of(refused).await,
            "model_id is required",
            "{method} {path}: a blank model names the field that is missing"
        );

        let long = format!(r#"{{"model_id":"{oversized}","secret_ref":"anthropic-prod"}}"#);
        let refused = send(ENTRIES_WRITE, method.clone(), path, Some(TENANT_KEY), &long).await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            detail_of(refused).await,
            "model_id must be at most 256 chars",
            "{method} {path}: an oversized model names the bound"
        );
    }
}

#[tokio::test]
async fn test_a_create_naming_no_credential_is_refused_before_the_store() {
    let refused = send(
        ENTRIES_WRITE,
        Method::POST,
        ENTRIES,
        Some(TENANT_KEY),
        r#"{"model_id":"claude-opus-5","secret_ref":""}"#,
    )
    .await;

    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(detail_of(refused).await, "secret_ref is required");
}

#[tokio::test]
async fn test_the_change_verb_cannot_be_asked_to_move_a_credential() {
    // `secret_ref` is not a field on the change body, and the create's presence
    // of one is what makes that worth pinning: a client sending both must not
    // silently retarget the credential. The extra key is IGNORED, matching the
    // Zig's `ignore_unknown_fields`, so the request proceeds on the model alone.
    let reached = send(
        ENTRIES_WRITE,
        Method::PATCH,
        ENTRY,
        Some(TENANT_KEY),
        r#"{"model_id":"claude-opus-5","secret_ref":"somewhere-else"}"#,
    )
    .await;

    assert_eq!(
        reached.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "the unknown field is ignored rather than refused"
    );
}

#[tokio::test]
async fn test_a_path_segment_that_is_not_an_identifier_never_reaches_a_pool() {
    // The refusal in front of the `::uuid` cast: a cast is not the place to
    // discover that a client sent something that is not an identifier.
    for method in [Method::PATCH, Method::DELETE] {
        let refused = send(
            ENTRIES_WRITE,
            method.clone(),
            "/v1/tenants/me/models/not-an-identifier",
            Some(TENANT_KEY),
            WELL_FORMED_UPDATE,
        )
        .await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "{method}");
        assert_eq!(detail_of(refused).await, "id must be a valid UUIDv7");
    }
}

#[tokio::test]
async fn test_the_page_size_is_bounded_at_both_ends() {
    for raw in ["0", "101", "-1", "", "ten", "1e2"] {
        let path = format!("{ENTRIES}?limit={raw}");
        let refused = send(ENTRIES_READ, Method::GET, &path, Some(TENANT_KEY), "").await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "limit {raw:?}");
        assert_eq!(
            code_of(refused).await,
            "UZ-LIBRARY-003",
            "limit {raw:?} is an input-bounds refusal, not a cursor one"
        );
    }
}

#[tokio::test]
async fn test_a_token_this_endpoint_never_issued_is_refused_as_malformed() {
    for raw in ["!!not-base64!!", "aGVsbG8", "eyJ2IjoxfQ"] {
        let path = format!("{ENTRIES}?starting_after={raw}");
        let refused = send(ENTRIES_READ, Method::GET, &path, Some(TENANT_KEY), "").await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "token {raw:?}");
        assert_eq!(code_of(refused).await, "UZ-LIBRARY-001", "token {raw:?}");
    }
}

#[tokio::test]
async fn test_a_real_token_for_another_walk_is_refused_distinctly() {
    // The split that keeps a cross-tenant replay attempt out of the same signal
    // as a truncated URL: this token DECODES, and it names a walk that is not
    // this one. Both halves of the binding are checked — another tenant, and
    // another page size.
    let foreign_tenant = token("0195b4ba-8d3a-7f13-8abc-cd00000000ff", 50);
    let path = format!("{ENTRIES}?starting_after={foreign_tenant}");
    let refused = send(ENTRIES_READ, Method::GET, &path, Some(TENANT_KEY), "").await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(refused).await, "UZ-LIBRARY-002");
    assert_eq!(
        detail_of(
            send(
                ENTRIES_READ,
                Method::GET,
                &format!("{ENTRIES}?starting_after={foreign_tenant}"),
                Some(TENANT_KEY),
                "",
            )
            .await
        )
        .await,
        "starting_after was issued for a different tenant or page size"
    );

    // A token minted under a different page size, replayed under this one.
    let other_size = token("0195b4ba-8d3a-7f13-8abc-cd00000000ff", 25);
    let path = format!("{ENTRIES}?limit=50&starting_after={other_size}");
    let refused = send(ENTRIES_READ, Method::GET, &path, Some(TENANT_KEY), "").await;
    assert_eq!(code_of(refused).await, "UZ-LIBRARY-002");
}

#[tokio::test]
async fn test_an_empty_resume_token_starts_the_walk_rather_than_refusing_it() {
    // `?starting_after=` is not a malformed cursor, it is no cursor — the same
    // reading the Zig gives it, and the difference between a first page and a
    // 400 for a client that always sends the parameter.
    let path = format!("{ENTRIES}?starting_after=");
    let reached = send(ENTRIES_READ, Method::GET, &path, Some(TENANT_KEY), "").await;
    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_the_collection_and_the_item_serve_different_method_sets() {
    // The route table's split, asserted at the edge: a PUT on either, and a
    // POST on the item, are the method router's refusal rather than a handler's.
    for (method, path) in [
        (Method::PUT, ENTRIES),
        (Method::PUT, ENTRY),
        (Method::POST, ENTRY),
        (Method::DELETE, ENTRIES),
    ] {
        let refused = send(ENTRIES_WRITE, method.clone(), path, Some(TENANT_KEY), "").await;
        assert_eq!(
            refused.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {path} is not a method this template serves"
        );
    }
}
