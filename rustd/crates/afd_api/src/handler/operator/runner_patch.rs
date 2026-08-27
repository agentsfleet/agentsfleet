//! Runner operator mutations over the authenticated tenant plane.

use std::borrow::Cow;
use std::sync::Arc;

use afd_wire::admin::{RunnerAdminAction, RunnerAdminPatchRequest, RunnerAdminPatchResponse};
use afd_wire::runner::AssignedPolicy;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

use super::query;
use crate::auth::PersonIdentity;
use crate::handler::{malformed, refuse};
use crate::services::Services;

const EVENT: &str = "runner_patch_failed";
const DETAIL_PATCH_BODY: &str = "PATCH body must be exactly one of {\"action\":\"cordon|drain|revoke|self_test\"} or {\"assigned_policy\":{sandbox_tier, network_policy, registry_allowlist[], worker_count, extra_binds[]}}";

enum Mutation<'a> {
    Transition(RunnerAdminAction),
    Selftest,
    Policy(&'a AssignedPolicy<'a>),
}

/// Applies one exactly-one-of runner mutation.
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(raw): Path<String>,
    body: Bytes,
) -> Response {
    let runner = match query::runner_id(&raw) {
        Ok(runner) => runner,
        Err(detail) => return malformed(detail),
    };
    let Ok(request) = afd_core::json::object_from_slice::<RunnerAdminPatchRequest<'_>>(&body)
    else {
        return malformed(DETAIL_PATCH_BODY);
    };
    let Some(mutation) = mutation(&request) else {
        return malformed(DETAIL_PATCH_BODY);
    };

    let result = match mutation {
        Mutation::Transition(action) => services
            .runners()
            .transition(&runner, action, services.now())
            .await
            .map(|admin_state| RunnerAdminPatchResponse {
                id: Cow::Borrowed(runner.as_str()),
                admin_state,
                assigned_policy: None,
                selftest_requested_at: None,
            }),
        Mutation::Selftest => services
            .runners()
            .request_selftest(&runner, services.now())
            .await
            .map(|recorded| RunnerAdminPatchResponse {
                id: Cow::Borrowed(runner.as_str()),
                admin_state: recorded.admin_state(),
                assigned_policy: None,
                selftest_requested_at: Some(recorded.requested_at()),
            }),
        Mutation::Policy(policy) => services
            .runners()
            .assign_policy(&runner, policy, services.now())
            .await
            .map(|stored| {
                let mut assigned_policy = policy.clone();
                assigned_policy.worker_count = stored.worker_count();
                RunnerAdminPatchResponse {
                    id: Cow::Borrowed(runner.as_str()),
                    admin_state: stored.admin_state(),
                    assigned_policy: Some(assigned_policy),
                    selftest_requested_at: None,
                }
            }),
    };

    match result {
        Ok(payload) => {
            tracing::debug!(
                actor_id = identity.0.subject().as_str(),
                runner_id = runner.as_str(),
                event = "runner_patched",
            );
            Json(payload).into_response()
        }
        Err(error) => {
            tracing::debug!(
                actor_id = identity.0.subject().as_str(),
                runner_id = runner.as_str(),
                error_code = error.code().as_str(),
                event = EVENT,
            );
            refuse(&error, EVENT)
        }
    }
}

fn mutation<'a>(request: &'a RunnerAdminPatchRequest<'a>) -> Option<Mutation<'a>> {
    match (request.action, request.assigned_policy.as_ref()) {
        (Some(RunnerAdminAction::SelfTest), None) => Some(Mutation::Selftest),
        (Some(action), None) => Some(Mutation::Transition(action)),
        (None, Some(policy)) => Some(Mutation::Policy(policy)),
        (None, None) | (Some(_), Some(_)) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_one_mutation_is_required() {
        let neither = RunnerAdminPatchRequest {
            action: None,
            assigned_policy: None,
        };
        assert!(mutation(&neither).is_none());

        let both = RunnerAdminPatchRequest {
            action: Some(RunnerAdminAction::Cordon),
            assigned_policy: Some(AssignedPolicy {
                sandbox_tier: afd_wire::runner::SandboxTier::DevNone,
                network_policy: afd_wire::runner::NetworkPolicy::AllowAll,
                registry_allowlist: Vec::new(),
                worker_count: 1,
                extra_binds: Vec::new(),
            }),
        };
        assert!(mutation(&both).is_none());
    }
}
