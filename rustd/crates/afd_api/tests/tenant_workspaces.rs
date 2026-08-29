//! The workspace directory's refusal matrix — everything in FRONT of the verbs.
//!
//! # Why this suite is about the refusals and not the rows
//!
//! The page is one statement and the create is an insert a unique index
//! arbitrates, so the harness answers the refusal a datastore that would not
//! answer gives, and what these tests pin is the guard, the scope rung, the
//! decoded query string, the cursor boundary, and the name rules: everything a
//! request meets before the pool is asked. The name-conflict 409 with its
//! `current_state` needs a real index to lose against — that proof rides the
//! integration lane.
//!
//! # The divergence is pinned where a client feels it
//!
//! A create naming NOTHING — no body, `{}`, a blank, whitespace however
//! spelled — REACHES the verb here, where the Zig daemon answers a 400. That
//! is the generate-on-absent divergence the spec's Discovery log records, and
//! these tests are what notice if it quietly regresses to the refusal.
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

/// The tenant-scoped list's path.
const WORKSPACES: &str = "/v1/tenants/me/workspaces";

/// The create's path.
const CREATE: &str = "/v1/workspaces";

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tdecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbad";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2workspaces";

/// What the route table demands of both verbs.
const WORKSPACE_ADMIN: ScopeSet = ScopeSet::from_scopes(&[Scope::WorkspaceAdmin]);

/// The empty set, proving a refusal below is the scope rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// The sentence the scope rung refuses with, as `Denied` renders it.
const DETAIL_SCOPE: &str = "Requires scope workspace:admin";

/// One request at a fresh router holding one scoped person.
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

/// Reads a problem document's `detail` back.
async fn detail_of(response: axum::response::Response) -> String {
    let document = harness::json_body(response).await;
    document
        .get("detail")
        .and_then(Value::as_str)
        .expect("every refusal carries a detail")
        .to_owned()
}

/// Asserts the request got PAST every layer in front of the verb — 503 with
/// the datastore sentence is the one answer only the SERVICE can produce.
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
async fn both_verbs_need_a_credential() {
    for (method, path) in [(Method::GET, WORKSPACES), (Method::POST, CREATE)] {
        let response = send(WORKSPACE_ADMIN, method, path, None, "").await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{path} is not anonymous"
        );
    }
}

#[tokio::test]
async fn both_verbs_need_the_workspace_admin_scope() {
    for (method, path) in [(Method::GET, WORKSPACES), (Method::POST, CREATE)] {
        let response = send(NO_SCOPES, method, path, Some(TENANT_KEY), "").await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{path} without workspace:admin is refused"
        );
        assert_eq!(detail_of(response).await, DETAIL_SCOPE, "{path}");
    }
}

#[tokio::test]
async fn a_scoped_person_reaches_the_list_verb() {
    let response = send(
        WORKSPACE_ADMIN,
        Method::GET,
        WORKSPACES,
        Some(TENANT_KEY),
        "",
    )
    .await;
    assert_reached_the_verb(response, "plain list").await;
}

#[tokio::test]
async fn every_wrong_limit_earns_the_one_sentence() {
    // One sentence for non-numeric and out-of-range alike — the workspace
    // list's vocabulary, where the charges walk spells two.
    for wrong in ["lots", "-1", "0", "101"] {
        let path = format!("{WORKSPACES}?limit={wrong}");
        let response = send(WORKSPACE_ADMIN, Method::GET, &path, Some(TENANT_KEY), "").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(
            detail_of(response).await,
            afd_api::handler::tenant::DETAIL_INVALID_LIMIT,
            "{path}"
        );
    }
}

#[tokio::test]
async fn the_limit_cap_is_inside_the_range() {
    let path = format!("{WORKSPACES}?limit=100");
    let response = send(WORKSPACE_ADMIN, Method::GET, &path, Some(TENANT_KEY), "").await;
    assert_reached_the_verb(response, "limit=100").await;
}

#[tokio::test]
async fn a_cursor_from_any_other_list_is_refused() {
    // Not a token, a text-sort token, and a timestamp token whose second half
    // is no workspace identifier — each is some OTHER list's cursor here.
    for foreign in [
        "!!not-a-token",
        "s:YWJj:01924f4e-0000-7000-8000-00000000beef",
        "1712924400000:not-a-uuid",
    ] {
        let path = format!("{WORKSPACES}?starting_after={foreign}");
        let response = send(WORKSPACE_ADMIN, Method::GET, &path, Some(TENANT_KEY), "").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(
            detail_of(response).await,
            afd_api::handler::tenant::DETAIL_INVALID_CURSOR,
            "{path}"
        );
    }
}

#[tokio::test]
async fn a_workspace_cursor_is_accepted() {
    let path = format!(
        "{WORKSPACES}?starting_after=1712924400000:{}",
        harness::OWNED_WORKSPACE
    );
    let response = send(WORKSPACE_ADMIN, Method::GET, &path, Some(TENANT_KEY), "").await;
    assert_reached_the_verb(response, "workspace cursor").await;
}

#[tokio::test]
async fn a_name_filter_outside_its_bounds_is_refused() {
    let long = "a".repeat(129);
    for wrong in ["", long.as_str()] {
        let path = format!("{WORKSPACES}?name={wrong}");
        let response = send(WORKSPACE_ADMIN, Method::GET, &path, Some(TENANT_KEY), "").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(
            detail_of(response).await,
            afd_api::handler::tenant::DETAIL_INVALID_NAME,
            "{path}"
        );
    }
}

#[tokio::test]
async fn a_percent_encoded_name_filter_is_decoded() {
    // `deploy%20bots` and `deploy+bots` both mean "deploy bots" — the decode
    // the sibling handlers skip is load-bearing HERE, because a workspace
    // name carries spaces. Reaching the verb is the proof the filter parsed.
    for spelled in ["deploy%20bots", "deploy+bots"] {
        let path = format!("{WORKSPACES}?name={spelled}");
        let response = send(WORKSPACE_ADMIN, Method::GET, &path, Some(TENANT_KEY), "").await;
        assert_reached_the_verb(response, spelled).await;
    }
}

#[tokio::test]
async fn a_broken_percent_escape_refuses_the_query_string() {
    for broken in ["name=a%2", "name=a%zz"] {
        let path = format!("{WORKSPACES}?{broken}");
        let response = send(WORKSPACE_ADMIN, Method::GET, &path, Some(TENANT_KEY), "").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(
            detail_of(response).await,
            afd_api::handler::tenant::DETAIL_MALFORMED_QUERY,
            "{path}"
        );
    }
}

#[tokio::test]
async fn choosing_no_name_reaches_the_verb_instead_of_a_400() {
    // The divergence itself: no body, an empty object, a null, and blankness
    // in two spellings all mean "name it for me" — the daemon generates
    // instead of refusing. On the Zig daemon every one of these is a 400.
    for body in [
        "",
        "{}",
        r#"{"name":null}"#,
        r#"{"name":"   "}"#,
        "{\"name\":\"\u{00a0}\"}",
    ] {
        let response = send(
            WORKSPACE_ADMIN,
            Method::POST,
            CREATE,
            Some(TENANT_KEY),
            body,
        )
        .await;
        assert_reached_the_verb(response, body).await;
    }
}

#[tokio::test]
async fn a_chosen_name_reaches_the_verb() {
    let response = send(
        WORKSPACE_ADMIN,
        Method::POST,
        CREATE,
        Some(TENANT_KEY),
        r#"{"name":"deploy bots"}"#,
    )
    .await;
    assert_reached_the_verb(response, "chosen name").await;
}

#[tokio::test]
async fn an_unknown_body_field_is_ignored_not_refused() {
    // `lifecycle.zig` parses with `ignore_unknown_fields = true`; a client
    // sending a field this daemon does not read must not start getting 400s.
    let response = send(
        WORKSPACE_ADMIN,
        Method::POST,
        CREATE,
        Some(TENANT_KEY),
        r#"{"name":"deploy bots","color":"teal"}"#,
    )
    .await;
    assert_reached_the_verb(response, "unknown field").await;
}

#[tokio::test]
async fn a_name_that_lies_about_itself_is_refused() {
    let response = send(
        WORKSPACE_ADMIN,
        Method::POST,
        CREATE,
        Some(TENANT_KEY),
        "{\"name\":\"evil\u{202e}name\"}",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        detail_of(response).await,
        afd_tenant::error::detail::DETAIL_WORKSPACE_NAME_INVALID,
        "a bidirectional override is refused for what it is"
    );
}

#[tokio::test]
async fn a_name_past_the_cap_is_refused() {
    let body = format!(r#"{{"name":"{}"}}"#, "a".repeat(129));
    let response = send(
        WORKSPACE_ADMIN,
        Method::POST,
        CREATE,
        Some(TENANT_KEY),
        &body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        detail_of(response).await,
        afd_tenant::error::detail::DETAIL_WORKSPACE_NAME_TOO_LONG,
    );
}

#[tokio::test]
async fn a_body_that_is_not_json_is_refused() {
    for broken in ["[", "[]", "\"name\""] {
        let response = send(
            WORKSPACE_ADMIN,
            Method::POST,
            CREATE,
            Some(TENANT_KEY),
            broken,
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{broken:?}");
        assert_eq!(
            detail_of(response).await,
            afd_api::handler::tenant::DETAIL_CREATE_BODY,
            "{broken:?}"
        );
    }
}
