//! The tenant runner routes' refusals in FRONT of the store: each route reads
//! the caller's tenant before any runner is read or written.

use crate::harness;
use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};

use self::harness::Fleet;

const RUNNERS: &str = "/v1/tenants/me/runners";
const RUNNER: &str = "/v1/tenants/me/runners/0199a801-1000-7000-8000-000000000001";
const SUBJECT: &str = "user_tenant_runners";
const RUNNER_TENANT: ScopeSet = ScopeSet::from_scopes(&[Scope::RunnerTenant]);

/// A tenant lookup that cannot answer is the outage class on every tenant
/// runner route: nothing is read, registered, changed or retired under a
/// guessed tenant.
// Value: protects=a tenant runner route never acts without the caller's
// tenant; fails_when=a route skips or swallows a failed tenant lookup;
// why_new=the tenant runner routes; seam=the production resolver over the
// harness's dead pool.
#[tokio::test]
async fn every_tenant_runner_route_refuses_when_the_tenant_cannot_be_read() {
    let router = Fleet::new()
        .with_live_ownership()
        .with_dashboard_holding(SUBJECT, RUNNER_TENANT)
        .router();
    for (method, path, body) in [
        (Method::POST, RUNNERS, "{}"),
        (Method::GET, RUNNERS, ""),
        (Method::GET, RUNNER, ""),
        (Method::PATCH, RUNNER, r#"{"action":"cordon"}"#),
        (Method::DELETE, RUNNER, ""),
    ] {
        let response = harness::send(&router, method.clone(), path, Some("session"), body).await;
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{method} {path}"
        );
    }
}

/// Once the tenant resolves, a registration whose body is not one is refused
/// before the store is asked.
#[tokio::test]
async fn a_tenant_registration_whose_body_is_not_one_is_refused() {
    let router = Fleet::new()
        .with_dashboard_holding(SUBJECT, RUNNER_TENANT)
        .router();
    let response = harness::send(
        &router,
        Method::POST,
        RUNNERS,
        Some("session"),
        "{not a registration",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
