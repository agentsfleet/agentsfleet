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
