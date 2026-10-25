//! Every runner route is usable by its runner principal.

use afd_auth::scope::RUNNER_SCOPES;
use http::Method;

/// The runner plane requires exactly one scope, and a runner always holds it.
///
/// Asserted against the catalogue rather than against a handler: the plane IS
/// the authorisation, so a runner-plane route that grew a second requirement
/// would be a route no runner could ever call.
#[test]
fn test_runner_scopes_satisfy_every_runner_route() {
    for route in afd_api::Route::all() {
        let meta = route.meta();
        if meta.guard != afd_api::Guard::RunnerBearer {
            continue;
        }
        for method in [Method::GET, Method::POST, Method::DELETE] {
            assert!(
                RUNNER_SCOPES.satisfies_any(meta.scopes.required(&method)),
                "{} requires a scope a runner principal does not hold",
                meta.template
            );
        }
    }
}

#[tokio::test]
async fn an_authorized_enrolment_reports_a_datastore_outage_without_minting_a_token() {
    use super::{ENROL_PATH, Fleet, OPERATOR, Scope, ScopeSet, TENANT_KEY, send};
    use afd_core::error_code;
    use http::StatusCode;

    let router = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::RunnerEnroll]),
        )
        .router();
    let payload = serde_json::json!({
        "host_id": "refused-host",
        "assigned_policy": {
            "sandbox_tier": "dev_none", "network_policy": "allow_all",
            "registry_allowlist": [], "worker_count": 1, "extra_binds": []
        },
        "labels": []
    });
    let response = send(
        &router,
        Method::POST,
        ENROL_PATH,
        Some(TENANT_KEY),
        &payload.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let document = super::harness::json_body(response).await;
    assert_eq!(
        document
            .get("error_code")
            .and_then(serde_json::Value::as_str),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
    assert!(document.get("runner_token").is_none());
}
