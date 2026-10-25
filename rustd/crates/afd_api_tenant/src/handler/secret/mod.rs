//! The workspace's secrets over HTTP: store, list, replace, delete.
//!
//! Four verbs across two templates, and not one of them returns a stored value:
//! a secret is write-only by contract, so there is no read handler here to have
//! got wrong.
//!
//! # The handler validates nothing, and that is the design
//!
//! A store needs an identifier check, a name check and a shape check, and a
//! replace needs two of the three; checks a handler has to remember are checks
//! a third verb could forget and still compile. Here each check is a CONSTRUCTOR:
//! [`SecretName::parse`] and [`SecretBody::parse`] answer with this daemon's
//! own registry codes, so the refusal a caller sees comes from the same table
//! whichever verb they reached it through.
//!
//! # Ownership is not checked here either
//!
//! It is a LAYER mounted from the route's own template, so
//! [`WorkspaceContext`] is a handler saying which workspace it is acting in —
//! never a handler deciding whether it may.

use std::borrow::Cow;
use std::sync::Arc;

use afd_api_wire::secret::{
    ReplaceSecretRequest, SecretsResponse, StoreSecretRequest, StoredSecretResponse,
};
use afd_connector::Forgotten;
use afd_vault::{Deleted, SecretBody, SecretName};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;
use serde::Deserialize;

use crate::auth::{Acting, WorkspaceContext};
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceConnectors as _, WorkspaceSecrets as _};

mod support;

use support::{connection_named, delete_refusal, may_disconnect, read_body, summary};

/// The scoped events each verb's failures are logged under.
const EVENT_STORE: &str = "secret_store_failed";
const EVENT_LIST: &str = "secret_list_failed";
const EVENT_REPLACE: &str = "secret_replace_failed";
const EVENT_DELETE: &str = "secret_delete_failed";

/// The state a still-referenced delete reports as its conflict — a secret's,
/// or a connection's whose credential a model entry names.
pub(crate) const STATE_REFERENCED: &str = "referenced";

/// The refusal a body this daemon cannot read earns.
pub const DETAIL_MALFORMED_JSON: &str = "Request body is not valid JSON";

/// The refusal a request with no body at all earns.
///
/// `MSG_BODY_REQUIRED`, kept verbatim: unlike the fleet install, an empty body
/// here cannot default to `{}` — there would be no secret to store.
pub const DETAIL_BODY_REQUIRED: &str = "request body is required";

/// The segment the item template carries.
///
/// A named struct rather than `Path<String>`, and that is a correctness fix
/// rather than documentation: the template carries TWO parameters, and
/// `Path<String>` deserializes a single one — it would fail the extractor and
/// answer a 500 before the handler ever ran.
#[derive(Debug, Deserialize)]
pub struct SecretPath {
    /// The secret named in the path, still text.
    pub name: String,
}

/// The body `store` answers a success with.
///
/// Named once, and named in the signature, so the handler and its
/// `#[utoipa::path]` annotation cannot drift apart without the binding test
/// below going red. `Response` erases this: `store` and `list` had identical
/// return types while answering different shapes, which is exactly how their
/// two annotations came to be swapped.
pub(crate) type StoredSecret = StoredSecretResponse<'static>;

/// `POST /v1/workspaces/{workspace_id}/secrets` — store one under a free name.
///
/// A name this workspace already holds is refused rather than overwritten, and
/// the decision is Postgres's: two concurrent creates on one name resolve to
/// one 201 and one 409 with no window in which both saw the name free.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/workspaces/{workspace_id}/secrets",
    tag = afd_http::openapi::tag::SECRETS,
    operation_id = "store_workspace_secret",
    summary = "Store a workspace secret",
    description = concat!(
        "Creates an encrypted secret; values are never returned. Send a nonempty JSON ",
        "object in `data`, up to 4 KiB. Invalid shapes return `UZ-VAULT-001`; oversized ",
        "values return `UZ-VAULT-002`.\n\n",
        "An existing name returns 409 `UZ-VAULT-005` without changing it. Concurrent ",
        "creates produce one 201 and one 409. Use the secret's `PUT` endpoint to replace ",
        "it.",
    ),
    request_body = StoreSecretRequest,
    params(
        afd_http::openapi::path::Workspace,
    ),
    responses(
        (status = 201, description = afd_http::openapi::CREATED, body = StoredSecretResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn store<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    body: Bytes,
) -> Result<(StatusCode, Json<StoredSecret>), Refusal> {
    let request = read_body::<StoreSecretRequest<'_>>(&body)?;
    // Parsed before the pool is touched, so a malformed request never draws a
    // connection and the refusal is the same whichever verb reached it.
    let name = SecretName::parse(&request.name).map_err(Refusal::at(EVENT_STORE))?;
    let secret = SecretBody::parse(request.data).map_err(Refusal::at(EVENT_STORE))?;

    services
        .secrets()
        .store(&owned.workspace, &name, &secret, services.now())
        .await
        .map_err(Refusal::at(EVENT_STORE))?;

    Ok((
        StatusCode::CREATED,
        Json(StoredSecretResponse {
            // Owned rather than borrowed: the name is parsed into a local, and
            // a typed return outlives it. Moved rather than copied: nothing
            // reads the local after this.
            name: Cow::Owned(name.into_string()),
        }),
    ))
}

/// `GET /v1/workspaces/{workspace_id}/secrets` — the descriptors, never a value.
///
/// Answers from the projection columns alone. No envelope is opened, and the
/// half of the store this reaches holds no key with which to open one.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/secrets",
    tag = afd_http::openapi::tag::SECRETS,
    operation_id = "list_workspace_secrets",
    summary = "List secrets stored for a workspace",
    description = concat!(
        "Returns secret names and non-secret details. Secret values are never ",
        "returned. ",
    ),
    params(
        afd_http::openapi::path::Workspace,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = SecretsResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
) -> Result<Response, Refusal> {
    let held = services
        .secrets()
        .list(&owned.workspace)
        .await
        .map_err(Refusal::at(EVENT_LIST))?;

    Ok(Json(SecretsResponse {
        secrets: held.iter().map(summary).collect(),
    })
    .into_response())
}

/// `PUT /v1/workspaces/{workspace_id}/secrets/{name}` — replace the whole body.
///
/// A name this workspace does not hold is a 404 and creates nothing: claiming a
/// name is [`store`]'s sole job, and an upsert here would resurrect a
/// credential a concurrent delete had just removed.
#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/v1/workspaces/{workspace_id}/secrets/{name}",
    tag = afd_http::openapi::tag::SECRETS,
    operation_id = "replace_workspace_secret",
    summary = "Replace a secret's stored body",
    description = concat!(
        "Replaces the entire secret with the JSON object in `data`. Omitted fields are ",
        "removed. Send every required field; stored secret values cannot be read back. A ",
        "missing name returns `UZ-VAULT-003` without creating it; use `POST` to create a ",
        "secret.",
    ),
    request_body = ReplaceSecretRequest,
    params(
        afd_http::openapi::path::Secret,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = StoredSecretResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn replace<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(SecretPath { name }): Path<SecretPath>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let name = SecretName::parse(&name).map_err(Refusal::at(EVENT_REPLACE))?;
    let request = read_body::<ReplaceSecretRequest<'_>>(&body)?;
    // The same shape gate the create applies, because it is the same
    // constructor. A replace that accepted a shape create rejects would let the
    // two verbs disagree about what a secret is.
    let secret = SecretBody::parse(request.data).map_err(Refusal::at(EVENT_REPLACE))?;

    services
        .secrets()
        .replace(&owned.workspace, &name, &secret)
        .await
        .map_err(Refusal::at(EVENT_REPLACE))?;

    Ok(Json(StoredSecretResponse {
        name: Cow::Borrowed(name.as_str()),
    })
    .into_response())
}

/// `DELETE /v1/workspaces/{workspace_id}/secrets/{name}` — remove one.
///
/// Idempotent 204: a name nothing is held under, and one a concurrent delete
/// removed first, both mean the credential is not there, which is what the
/// request asked for.
///
/// A credential the tenant's model registry still names is a 409 carrying the
/// COUNT. The count comes from the statement that locked those entries, so it
/// cannot have changed between the decision and this sentence.
///
/// A name that is a connector's grant key IS that connection, so it is
/// disconnected: a bare vault delete would skip the workspace's turn and leave
/// routing rows naming an account with no credential behind them.
/// It needs `connector:write` beside the route's `secret:write`, the scope
/// Disconnect's own route asks for.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/workspaces/{workspace_id}/secrets/{name}",
    tag = afd_http::openapi::tag::SECRETS,
    operation_id = "delete_workspace_secret",
    summary = "Delete a secret from the workspace vault",
    description = concat!(
        "Deletes a secret; missing secrets also return 204. Models still using it cause 409 ",
        "`UZ-VAULT-004`, with the number of references. Deleting a connector's `{provider}` ",
        "secret also removes its event routing and requires `connector:write`.",
    ),
    params(
        afd_http::openapi::path::Secret,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn remove<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Acting(principal): Acting,
    Path(SecretPath { name }): Path<SecretPath>,
) -> Result<Response, Refusal> {
    let name = SecretName::parse(&name).map_err(Refusal::at(EVENT_DELETE))?;
    if let Some(provider) = connection_named(&name) {
        may_disconnect(&principal)?;
        // The connector's own Disconnect, answered as this route answers.
        return match services
            .connectors()
            .forget(&owned.workspace, provider)
            .await
        {
            Ok(Forgotten::Disconnected | Forgotten::AlreadyAbsent) => {
                Ok(StatusCode::NO_CONTENT.into_response())
            }
            Err(refused) => Err(delete_refusal(refused.referenced_by(), refused)),
        };
    }

    match services.secrets().remove(&owned.workspace, &name).await {
        Ok(Deleted::Removed | Deleted::AlreadyAbsent) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(refused) => Err(delete_refusal(refused.referenced_by(), refused)),
    }
}

#[cfg(all(test, feature = "openapi"))]
mod contract;
