//! The provider view and reset's refusal matrix — everything in FRONT of the
//! verbs, plus the one refusal only the reset itself can answer.
//!
//! The activation's first ladder rung is proven here too — it refuses BEFORE
//! any pool is touched, which is the whole reason `secret_ref` is optional on
//! the wire rather than required by serde.
//!
//! # Why the datastore's refusal is the success signal
//!
//! Both verbs open with reads a real Postgres would evaluate, so over the
//! harness's unreachable pool, "reached the verb" renders as the 503 only the
//! SERVICE can produce — the guard, the scope rung and the method router all
//! refuse with codes of their own before any pool is touched.

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

/// The one template all three verbs share.
const PROVIDER: &str = "/v1/tenants/me/provider";

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tfeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedface";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2provider";

/// What the route table demands of the read.
const PROVIDER_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::SecretRead]);

/// What it demands of the mutations.
const PROVIDER_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::SecretWrite]);

/// The empty set, proving a refusal below is the scope rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// A request at the provider template, against a fresh router.
async fn send(
    scopes: ScopeSet,
    method: Method,
    credential: Option<&str>,
) -> axum::response::Response {
    send_body(scopes, method, credential, "").await
}

/// The same, carrying a body.
async fn send_body(
    scopes: ScopeSet,
    method: Method,
    credential: Option<&str>,
    body: &str,
) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, scopes)
        .router();
    harness::send(&router, method, PROVIDER, credential, body).await
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

#[tokio::test]
async fn test_the_view_and_the_reset_sit_behind_the_bearer_guard() {
    for method in [Method::GET, Method::DELETE] {
        let anonymous = send(PROVIDER_READ, method.clone(), None).await;
        assert_eq!(
            anonymous.status(),
            StatusCode::UNAUTHORIZED,
            "{method} with no credential is the guard's refusal, not a 404"
        );
    }
}

#[tokio::test]
async fn test_the_scope_rung_separates_the_read_from_the_reset() {
    // A read-scoped credential may view and may not reset; an unscoped one
    // may do neither. The rung, not the handler, is what answers — the pool
    // behind the harness cannot answer anything.
    let scoped_read = send(PROVIDER_READ, Method::DELETE, Some(TENANT_KEY)).await;
    assert_eq!(
        scoped_read.status(),
        StatusCode::FORBIDDEN,
        "secret:read alone must not authorize a reset"
    );

    let unscoped = send(NO_SCOPES, Method::GET, Some(TENANT_KEY)).await;
    assert_eq!(
        unscoped.status(),
        StatusCode::FORBIDDEN,
        "no scopes, no view"
    );
}

#[tokio::test]
async fn test_both_verbs_reach_their_service_over_the_dead_pool() {
    // 503 with the datastore sentence is what "past every refusal layer"
    // renders as over a pool that answers nothing — see the module note.
    for (method, scopes) in [
        (Method::GET, PROVIDER_READ),
        (Method::DELETE, PROVIDER_WRITE),
    ] {
        let reached = send(scopes, method.clone(), Some(TENANT_KEY)).await;
        assert_eq!(
            reached.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{method}: only the verb answers with the datastore's refusal"
        );
        assert_eq!(
            detail_of(reached).await,
            "Database unavailable",
            "{method}: the sentence is the credential plane's outage detail"
        );
    }
}

#[tokio::test]
async fn test_a_body_this_daemon_cannot_read_never_reaches_a_pool() {
    let refused = send_body(PROVIDER_WRITE, Method::PUT, Some(TENANT_KEY), "{").await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_self_managed_naming_no_credential_is_the_ladder_s_first_rung() {
    // Rung one, and it is the reason `secret_ref` is optional on the wire: a
    // serde-required field would answer a shape error naming no registry code,
    // where this answers UZ-PROVIDER-001 with a sentence a client can act on.
    // It refuses before the pool is touched, which the dead harness proves —
    // anything reaching the store would answer 503 instead.
    let refused = send_body(
        PROVIDER_WRITE,
        Method::PUT,
        Some(TENANT_KEY),
        r#"{"mode":"self_managed"}"#,
    )
    .await;

    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(refused).await, "UZ-PROVIDER-001");
}

#[tokio::test]
async fn test_the_platform_arm_of_a_put_is_the_reset() {
    // Byte-equivalent to DELETE by construction — one function serves both —
    // so it reaches the same service and answers the same outage.
    let reached = send_body(
        PROVIDER_WRITE,
        Method::PUT,
        Some(TENANT_KEY),
        r#"{"mode":"platform"}"#,
    )
    .await;

    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(detail_of(reached).await, "Database unavailable");
}

#[tokio::test]
async fn test_a_self_managed_activation_reaches_its_transaction() {
    // Past rung one, so the next thing it meets is the pool that answers
    // nothing — proving the ladder's remaining rungs are the store's, not the
    // handler's.
    let reached = send_body(
        PROVIDER_WRITE,
        Method::PUT,
        Some(TENANT_KEY),
        r#"{"mode":"self_managed","secret_ref":"my-key"}"#,
    )
    .await;

    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_a_reset_on_a_deployment_with_no_default_names_the_missing_key() {
    // The one refusal on this surface decided by a row nothing else can
    // arrange. `core.platform_provider_defaults` has no tenant column, so the
    // integration lane's shared database cannot assert the table is empty, and
    // over the dead pool the same read answers 503 rather than the `None` this
    // arm needs. The seam supplies that `None` and everything else is real:
    // real router, real guard, real scope rung, real handler.
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, PROVIDER_WRITE)
        .without_platform_default()
        .router();
    let refused = harness::send(&router, Method::DELETE, PROVIDER, Some(TENANT_KEY), "").await;

    // 500, not 400, and the registry says why: this is a deployment that was
    // never configured, so there is nothing the CALLER can change. The status
    // is the difference between "you asked wrongly" and "an operator must set
    // a platform default", and a 4xx here would send the tenant looking for a
    // mistake of their own.
    assert_eq!(refused.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(code_of(refused).await, "UZ-PROVIDER-009");

    let sentence = harness::send(&router, Method::DELETE, PROVIDER, Some(TENANT_KEY), "").await;
    assert_eq!(
        detail_of(sentence).await,
        "Platform LLM key not configured",
        "the operator repair is naming the missing key, not the tenant's request"
    );
}

#[tokio::test]
async fn test_a_reset_reaches_its_write_when_a_default_is_configured() {
    // The control the case above is read against. Same route, same scope, same
    // credential — and without the substitution the read is the real store's,
    // which over the dead pool is the datastore's refusal rather than
    // `UZ-PROVIDER-009`. A handler that answered the missing-key code
    // unconditionally would pass the case above and fail this one.
    let reached = send(PROVIDER_WRITE, Method::DELETE, Some(TENANT_KEY)).await;

    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_the_write_scope_gates_the_activation() {
    let read_only = send_body(
        PROVIDER_READ,
        Method::PUT,
        Some(TENANT_KEY),
        r#"{"mode":"platform"}"#,
    )
    .await;
    assert_eq!(read_only.status(), StatusCode::FORBIDDEN);
}
