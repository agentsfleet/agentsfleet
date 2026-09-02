//! Runner operator mutations over the authenticated tenant plane.

use std::borrow::Cow;
use std::sync::Arc;

use afd_wire::admin::{
    RunnerAdminAction, RunnerAdminPatchRequest, RunnerAdminPatchResponse,
    RunnerTokenRotatedResponse,
};
use afd_wire::runner::AssignedPolicy;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::header;

use super::query;
use crate::auth::PersonIdentity;
use crate::handler::{malformed, refuse};
use crate::services::Services;

const EVENT: &str = "runner_patch_failed";
const DETAIL_PATCH_BODY: &str = "PATCH body must be exactly one of {\"action\":\"cordon|drain|revoke|rotate|self_test\"} or {\"assigned_policy\":{sandbox_tier, network_policy, registry_allowlist[], worker_count, extra_binds[]}}";

enum Mutation<'a> {
    Transition(RunnerAdminAction),
    Rotate,
    Selftest,
    Policy(&'a AssignedPolicy<'a>),
}

enum OrdinaryMutation<'a> {
    Transition(RunnerAdminAction),
    Selftest,
    Policy(&'a AssignedPolicy<'a>),
}

/// Applies one exactly-one-of runner mutation.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/fleets/runners/{runner_id}",
    tag = afd_http::openapi::tag::FLEET,
    operation_id = "patch_fleet_runner",
    summary = "Administer a fleet runner",
    description = concat!(
        "Platform-admin mutation on a single runner. The body carries exactly ",
        "one of `action` or `assigned_policy`. `action` moves the admin ",
        "state: `cordon` to cordoned, `drain` to draining, `revoke` to ",
        "revoked. Revoked is terminal; a revoked runner cannot transition ",
        "back. `self_test` moves no state. It records a request for the ",
        "runner to test its own sandbox. The host picks the request up on its ",
        "next heartbeat and answers on a later one. The reply is the recorded ",
        "request, never a verdict. A revoked runner refuses it with 409 `UZ- ",
        "RUN-018`, since it will never heartbeat again. `assigned_policy` re- ",
        "assigns the runner's policy, and the host picks it up on its next ",
        "heartbeat. Sending the same policy again changes nothing and records ",
        "no event. ",
    ),
    request_body = RunnerAdminPatchRequest,
    params(
        afd_http::openapi::path::Runner,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = RunnerAdminPatchResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
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

    let ordinary = match mutation {
        Mutation::Rotate => return rotate(&*services, &identity, &runner).await,
        Mutation::Transition(action) => OrdinaryMutation::Transition(action),
        Mutation::Selftest => OrdinaryMutation::Selftest,
        Mutation::Policy(policy) => OrdinaryMutation::Policy(policy),
    };
    let result = mutate(&*services, &runner, identity.subject(), ordinary).await;
    finish(result, &identity, &runner)
}

async fn mutate<'a, D: Services>(
    services: &D,
    runner: &'a afd_core::id::Uuid7,
    actor: &str,
    mutation: OrdinaryMutation<'a>,
) -> afd_runner::Result<RunnerAdminPatchResponse<'a>> {
    match mutation {
        OrdinaryMutation::Transition(action) => services
            .runners()
            .transition(runner, action, actor, services.now())
            .await
            .map(|admin_state| RunnerAdminPatchResponse {
                id: Cow::Borrowed(runner.as_str()),
                admin_state,
                assigned_policy: None,
                selftest_requested_at: None,
            }),
        OrdinaryMutation::Selftest => services
            .runners()
            .request_selftest(runner, services.now())
            .await
            .map(|recorded| RunnerAdminPatchResponse {
                id: Cow::Borrowed(runner.as_str()),
                admin_state: recorded.admin_state(),
                assigned_policy: None,
                selftest_requested_at: Some(recorded.requested_at()),
            }),
        OrdinaryMutation::Policy(policy) => services
            .runners()
            .assign_policy(runner, policy, services.now())
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
    }
}

fn finish(
    result: afd_runner::Result<RunnerAdminPatchResponse<'_>>,
    identity: &PersonIdentity,
    runner: &afd_core::id::Uuid7,
) -> Response {
    match result {
        Ok(payload) => {
            let actor_id = identity.subject();
            let runner_id = runner.as_str();
            tracing::info!(actor_id, runner_id, event = "runner_patched",);
            Json(payload).into_response()
        }
        Err(error) => {
            let actor_id = identity.subject();
            let runner_id = runner.as_str();
            let error_code = error.code().as_str();
            tracing::debug!(actor_id, runner_id, error_code, event = EVENT,);
            refuse(&error, EVENT)
        }
    }
}

async fn rotate<D: Services>(
    services: &D,
    identity: &PersonIdentity,
    runner: &afd_core::id::Uuid7,
) -> Response {
    match services
        .runners()
        .rotate_token(runner, identity.subject(), services.now())
        .await
    {
        Ok(rotated) => {
            let actor_id = identity.subject();
            let runner_id = runner.as_str();
            tracing::info!(actor_id, runner_id, event = "runner_token_rotated",);
            (
                [(header::CACHE_CONTROL, "no-store")],
                Json(RunnerTokenRotatedResponse {
                    id: Cow::Borrowed(runner.as_str()),
                    runner_token: Cow::Borrowed(rotated.expose()),
                }),
            )
                .into_response()
        }
        Err(error) => refuse_rotation(&error, identity, runner),
    }
}

fn mutation<'a>(request: &'a RunnerAdminPatchRequest<'a>) -> Option<Mutation<'a>> {
    match (request.action, request.assigned_policy.as_ref()) {
        (Some(RunnerAdminAction::SelfTest), None) => Some(Mutation::Selftest),
        (Some(RunnerAdminAction::Rotate), None) => Some(Mutation::Rotate),
        (Some(action), None) => Some(Mutation::Transition(action)),
        (None, Some(policy)) => Some(Mutation::Policy(policy)),
        (None, None) | (Some(_), Some(_)) => None,
    }
}

fn refuse_rotation(
    error: &afd_runner::Error,
    identity: &PersonIdentity,
    runner: &afd_core::id::Uuid7,
) -> Response {
    let actor_id = identity.subject();
    let runner_id = runner.as_str();
    let error_code = error.code().as_str();
    tracing::debug!(actor_id, runner_id, error_code, event = EVENT,);
    refuse(error, EVENT)
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
