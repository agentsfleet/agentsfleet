//! Runner operator mutations over the authenticated tenant plane.

use std::borrow::Cow;
use std::sync::Arc;

use afd_api_wire::admin::{
    RunnerAdminAction, RunnerAdminPatchRequest, RunnerAdminPatchResponse,
    RunnerTokenRotatedResponse,
};
use afd_runner::RunnerEdit;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::header;

use super::query;
use crate::auth::PersonIdentity;
use crate::handler::{malformed, parse_id, refuse};
use crate::services::Services;

const EVENT: &str = "runner_patch_failed";
const DETAIL_PATCH_BODY: &str = "PATCH body must be {\"action\":\"cordon|drain|revoke|rotate|self_test\"} alone, or an edit carrying {\"assigned_policy\":{sandbox_tier, network_policy, registry_allowlist[], worker_count, extra_binds[]}}, {\"labels\":[]}, or both";

enum Mutation<'r, 'a> {
    Transition(RunnerAdminAction),
    Rotate,
    Selftest,
    Edit(RunnerEdit<'r, 'a>),
}

enum OrdinaryMutation<'r, 'a> {
    Transition(RunnerAdminAction),
    Selftest,
    Edit(RunnerEdit<'r, 'a>),
}

/// Applies an action alone, or an edit of the policy, the labels, or both.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/fleets/runners/{runner_id}",
    tag = afd_http::openapi::tag::FLEET,
    operation_id = "patch_fleet_runner",
    summary = "Administer a fleet runner",
    description = concat!(
        "Platform-admin mutation on a single runner. The body carries either ",
        "`action` alone, or an edit: `assigned_policy`, `labels`, or both. An ",
        "edit carrying both applies both or neither. `action` moves the admin ",
        "state: `cordon` to cordoned, `drain` to draining, `revoke` to ",
        "revoked. Revoked is terminal; a revoked runner cannot transition ",
        "back. `self_test` moves no state. It records a request for the ",
        "runner to test its own sandbox. The host picks the request up on its ",
        "next heartbeat and answers on a later one. The reply is the recorded ",
        "request, never a verdict. A revoked runner refuses it with 409 ",
        "`UZ-RUN-018`, since it will never heartbeat again. `assigned_policy` ",
        "reassigns the runner's policy, and the host picks it up on its next ",
        "heartbeat. Sending the same policy again changes nothing and records ",
        "no event. A revoked runner refuses a policy with 400. `labels` ",
        "replaces the runner's whole label set, the labels a fleet's required ",
        "tags are matched against on the runner's next lease poll. The bounds ",
        "and the refusal are the ones enrollment applies. A label given twice ",
        "is stored once, in the order first given, an empty list clears every ",
        "label, and a revoked runner's labels stay editable. The reply carries ",
        "`labels` as stored whenever the request sent them. ",
    ),
    request_body = RunnerAdminPatchRequest,
    params(
        afd_http::openapi::path::Runner,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = RunnerAdminPatchResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = "The runner is revoked, so the request can never be honoured"),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
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
    let runner = match parse_id(&raw, query::DETAIL_BAD_RUNNER_ID) {
        Ok(runner) => runner,
        Err(refused) => return refused.into_response(),
    };
    // Strict: the assigned policy is a shape the runner reads leniently, and an
    // operator's misspelled key inside it must still be refused.
    let Ok(request) = afd_http::handler::read_strict_body::<RunnerAdminPatchRequest<'_>>(&body)
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
        Mutation::Edit(edit) => OrdinaryMutation::Edit(edit),
    };
    let result = mutate(&*services, &runner, identity.subject(), ordinary).await;
    finish(result, &identity, &runner)
}

async fn mutate<'a, D: Services>(
    services: &D,
    runner: &'a afd_core::id::Uuid7,
    actor: &str,
    mutation: OrdinaryMutation<'_, 'a>,
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
                labels: None,
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
                labels: None,
                selftest_requested_at: Some(recorded.requested_at()),
            }),
        OrdinaryMutation::Edit(edit) => services
            .runners()
            .edit(runner, edit, services.now())
            .await
            .map(|stored| RunnerAdminPatchResponse {
                id: Cow::Borrowed(runner.as_str()),
                admin_state: stored.admin_state,
                assigned_policy: stored.assigned_policy,
                labels: stored.labels,
                selftest_requested_at: None,
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

/// An action travels alone; an edit carries a policy, labels, or both.
fn mutation<'r, 'a>(request: &'r RunnerAdminPatchRequest<'a>) -> Option<Mutation<'r, 'a>> {
    let edit = RunnerEdit {
        assigned_policy: request.assigned_policy.as_ref(),
        labels: request.labels.as_deref(),
    };
    let editing = edit.assigned_policy.is_some() || edit.labels.is_some();
    match (request.action, editing) {
        (Some(RunnerAdminAction::SelfTest), false) => Some(Mutation::Selftest),
        (Some(RunnerAdminAction::Rotate), false) => Some(Mutation::Rotate),
        (Some(action), false) => Some(Mutation::Transition(action)),
        (None, true) => Some(Mutation::Edit(edit)),
        (None, false) | (Some(_), true) => None,
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

    fn policy() -> afd_wire::runner::AssignedPolicy<'static> {
        afd_wire::runner::AssignedPolicy {
            sandbox_tier: afd_wire::runner::SandboxTier::DevNone,
            network_policy: afd_wire::runner::NetworkPolicy::AllowAll,
            registry_allowlist: Vec::new(),
            worker_count: 1,
            extra_binds: Vec::new(),
        }
    }

    fn request(
        action: Option<RunnerAdminAction>,
        policy: bool,
        labels: bool,
    ) -> RunnerAdminPatchRequest<'static> {
        RunnerAdminPatchRequest {
            action,
            assigned_policy: policy.then(self::policy),
            labels: labels.then(|| vec![Cow::Borrowed("gpu")]),
        }
    }

    /// The name of the mutation a body reads as, or `None` for a refusal.
    fn read_as(request: &RunnerAdminPatchRequest<'_>) -> Option<&'static str> {
        mutation(request).map(|mutation| match mutation {
            Mutation::Transition(_) => "transition",
            Mutation::Rotate => "rotate",
            Mutation::Selftest => "self-test",
            Mutation::Edit(edit) => match (edit.assigned_policy, edit.labels) {
                (Some(_), Some(_)) => "edit both",
                (Some(_), None) => "edit policy",
                (None, _) => "edit labels",
            },
        })
    }

    #[test]
    fn an_action_travels_alone_and_an_edit_carries_policy_labels_or_both() {
        let cordon = Some(RunnerAdminAction::Cordon);
        let cases = [
            (request(None, false, false), None),
            (request(cordon, false, false), Some("transition")),
            (
                request(Some(RunnerAdminAction::Rotate), false, false),
                Some("rotate"),
            ),
            (
                request(Some(RunnerAdminAction::SelfTest), false, false),
                Some("self-test"),
            ),
            (request(None, true, false), Some("edit policy")),
            (request(None, false, true), Some("edit labels")),
            (request(None, true, true), Some("edit both")),
            (request(cordon, true, false), None),
            (request(cordon, false, true), None),
            (request(cordon, true, true), None),
            (request(Some(RunnerAdminAction::Rotate), false, true), None),
        ];
        for (body, expected) in cases {
            assert_eq!(read_as(&body), expected, "{body:?}");
        }
    }

    /// An empty list is an edit that clears, never the absence of one.
    #[test]
    fn an_empty_label_list_is_an_edit() {
        let clear = RunnerAdminPatchRequest {
            action: None,
            assigned_policy: None,
            labels: Some(Vec::new()),
        };
        assert_eq!(read_as(&clear), Some("edit labels"));
    }

    /// The assigned policy is a shape the runner reads leniently; an operator's
    /// misspelled key inside it is still refused, so no policy lands half-read.
    #[test]
    fn a_misspelled_key_inside_the_assigned_policy_is_refused() {
        let body = br#"{"action":null,"assigned_policy":{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":1,"extra_binds":[],"wroker_count":4}}"#;

        let refused = afd_http::handler::read_strict_body::<RunnerAdminPatchRequest<'_>>(body)
            .err()
            .and_then(|error| afd_core::json::unknown_field_of(&error));

        // `?` is how `serde_ignored` spells the `Option` the policy rides in.
        assert_eq!(refused.as_deref(), Some("assigned_policy.?.wroker_count"));
    }
}
