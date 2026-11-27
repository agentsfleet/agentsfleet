//! A platform operator rotates one workspace's checkpoint key.

use std::sync::Arc;

use af_core::error_code;
use af_wire::durable_task::CheckpointKeyRotation;
use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

use crate::OperatorPlane;
use crate::auth::PersonIdentity;
use crate::handler::{parse_id, refuse, reject};

const EVENT_FAILED: &str = "admin_checkpoint_key_rotate_failed";
const DETAIL_WORKSPACE_ID: &str = "workspace_id must be a canonical UUIDv7";

/// Writes a new checkpoint key for the workspace and retires every checkpoint
/// the old key sealed, in one transaction.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/admin/workspaces/{workspace_id}/checkpoint-key/rotate",
    tag = afd_http::openapi::tag::ADMIN,
    operation_id = "rotate_workspace_checkpoint_key",
    summary = "Rotate a workspace's checkpoint key",
    description = concat!(
        "A side-effecting operation with no undo: writes a new checkpoint key for the ",
        "workspace and deletes every checkpoint the old key sealed. Their stored objects ",
        "are removed from object storage afterwards. A run interrupted across the ",
        "rotation starts from its original message, and the workspace's next save is a ",
        "full one. Use it when a key may be compromised. Sends no body and honors no ",
        "`Idempotency-Key`: a repeat rotates again and retires what was saved in between. ",
        "A workspace the caller cannot reach, an unknown one among them, returns 403 ",
        "`UZ-AUTH-001`.",
    ),
    params(
        ("workspace_id" = String, Path, description = "The workspace whose key rotates."),
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = CheckpointKeyRotation),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn rotate<D: OperatorPlane>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(raw): Path<String>,
) -> Response {
    let workspace = match parse_id(&raw, DETAIL_WORKSPACE_ID) {
        Ok(workspace) => workspace,
        Err(refused) => return refused.into_response(),
    };
    match services.checkpoint_keys().rotate(&workspace).await {
        Ok(Some(retired)) => {
            let (actor_id, workspace_id) = (identity.subject(), workspace.as_str());
            let (generations, objects) = (retired.generations, retired.objects);
            tracing::info!(
                event = "admin_checkpoint_key_rotated",
                actor_id,
                workspace_id,
                generations,
                objects
            );
            Json(CheckpointKeyRotation {
                retired_generations: retired.generations,
                retired_objects: retired.objects,
            })
            .into_response()
        }
        // The workspace was deleted after the ownership check passed: answered
        // as that check answers a workspace the caller cannot reach.
        Ok(None) => reject(error_code::AUTH_FORBIDDEN, afd_http::auth::DETAIL_NOT_YOURS),
        Err(error) => refuse(&error, EVENT_FAILED),
    }
}
