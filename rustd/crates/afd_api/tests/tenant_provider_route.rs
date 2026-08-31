//! The provider view and reset's refusal matrix — everything in FRONT of the
//! verbs, plus the one refusal only the reset itself can answer.
//!
//! The activation (`PUT`) joins this file when its verb lands; the guard and
//! scope rungs proven here are the same layers it will sit behind.
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
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, scopes)
        .router();
    harness::send(&router, method, PROVIDER, credential, "").await
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
async fn test_the_activation_method_is_tabled_but_not_yet_served() {
    // PUT mounts with the activation verb. Until then the method router
    // answers 405 — the path exists, the method is not yet served — and this
    // test flips to a service assertion the day it lands.
    let put = send(PROVIDER_WRITE, Method::PUT, Some(TENANT_KEY)).await;
    assert_eq!(put.status(), StatusCode::METHOD_NOT_ALLOWED);
}
