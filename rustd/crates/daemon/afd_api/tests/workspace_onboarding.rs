//! The onboarding surface, in front of the store.
//!
//! What a request meets before Postgres: the ownership layer and the scope
//! posture the route carries (`NONE` — it is about the caller's own workspace,
//! not about what the caller may reach). The checklist turning over as the
//! workspace fills up rides the integration lane, because it is five `EXISTS`
//! subqueries.
#![cfg(feature = "test-util")]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::Method;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tdeadbeefdecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbad";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2onboarding";

/// A set with something in it, to prove the rung is not silently demanding one.
const SOME_SCOPE: ScopeSet = ScopeSet::from_scopes(&[Scope::SecretRead]);

/// The onboarding checklist, under the workspace the fixture owns.
fn onboarding() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/onboarding")
}

/// One request at a fresh router holding one scoped person.
async fn send(scopes: ScopeSet, path: &str) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, scopes)
        .router();
    harness::send(&router, Method::GET, path, Some(TENANT_KEY), "").await
}

/// Holding a scope this surface never asks for changes nothing.
///
/// The route carries `Scopes::Always(NONE)`, so a caller with an unrelated
/// scope is admitted exactly as one with none — proving the rung is not
/// quietly demanding something the route table says it does not.
#[tokio::test]
async fn an_unrelated_scope_neither_admits_nor_refuses() {
    let with_none = send(ScopeSet::EMPTY, &onboarding()).await;
    let with_some = send(SOME_SCOPE, &onboarding()).await;
    assert_eq!(
        with_none.status(),
        with_some.status(),
        "an unrelated scope must not change the verdict"
    );
}
