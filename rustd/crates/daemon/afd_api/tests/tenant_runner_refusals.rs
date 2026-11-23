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

/// An empty registration is told the body is required, a sentence apart from
/// the one a body that does not parse earns.
#[tokio::test]
async fn an_empty_tenant_registration_is_told_its_body_is_required() {
    let router = Fleet::new()
        .with_dashboard_holding(SUBJECT, RUNNER_TENANT)
        .router();
    let empty = harness::send(&router, Method::POST, RUNNERS, Some("session"), "").await;
    assert_eq!(empty.status(), StatusCode::BAD_REQUEST);
    assert_eq!(harness::detail_of(empty).await, "Request body required");
    let unreadable = harness::send(&router, Method::POST, RUNNERS, Some("session"), "{").await;
    assert_eq!(unreadable.status(), StatusCode::BAD_REQUEST);
    assert_ne!(
        harness::detail_of(unreadable).await,
        "Request body required"
    );
}

/// A caller whose credential resolves to no tenant is refused on every tenant
/// runner route, as a 403 that says why, before any runner is read.
#[tokio::test]
async fn a_caller_with_no_tenant_is_refused_on_every_tenant_runner_route() {
    let fleet = Fleet::new().with_dashboard_holding(SUBJECT, RUNNER_TENANT);
    fleet.ownership().revoke();
    let router = fleet.router();
    for (method, path, body) in [
        (Method::GET, RUNNERS, ""),
        (Method::GET, RUNNER, ""),
        (Method::DELETE, RUNNER, ""),
    ] {
        let response = harness::send(&router, method.clone(), path, Some("session"), body).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method} {path}");
        let problem = harness::json_body(response).await;
        assert_eq!(
            harness::error_code(&problem),
            Some(af_core::error_code::AUTH_FORBIDDEN.as_str()),
            "{method} {path}"
        );
        assert_eq!(
            harness::text(&problem, "detail"),
            Some(
                "This account isn't part of a tenant, so it has no runners to manage. \
                 Sign in with your tenant account."
            ),
            "{method} {path}"
        );
    }
}
