//! The workspace's secrets over HTTP: store, list, replace, delete.
//!
//! The port of `fleets/secrets.zig` and `fleets/secret_list.zig`. Four verbs
//! across two templates, and not one of them returns a stored value: a secret
//! is write-only by contract, so there is no read handler here to have got
//! wrong.
//!
//! # The handler validates nothing, and that is the design
//!
//! `innerStoreSecret` opens with an identifier check, a name check and a shape
//! check, and `innerReplaceSecret` repeats two of the three — a third verb that
//! forgot either would compile. Here each check is a CONSTRUCTOR:
//! [`SecretName::parse`] and [`SecretBody::parse`] answer with this daemon's
//! own registry codes, so the refusal a caller sees comes from the same table
//! whichever verb they reached it through.
//!
//! # Ownership is not checked here either
//!
//! Every Zig handler in this family opens with a `workspace_guards.enforce`
//! call. Here it is a LAYER mounted from the route's own template, so
//! [`WorkspaceContext`] is a handler saying which workspace it is acting in —
//! never a handler deciding whether it may.

use std::borrow::Cow;
use std::sync::Arc;

use afd_vault::{Deleted, SecretBody, SecretName, SecretSummary};
use afd_wire::secret::{
    ReplaceSecretRequest, SecretsResponse, StoreSecretRequest, StoredSecretResponse,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;
use serde::Deserialize;

use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceSecrets as _};

/// The scoped events each verb's failures are logged under.
const EVENT_STORE: &str = "secret_store_failed";
const EVENT_LIST: &str = "secret_list_failed";
const EVENT_REPLACE: &str = "secret_replace_failed";
const EVENT_DELETE: &str = "secret_delete_failed";

/// The state a still-referenced delete reports as its conflict.
const STATE_REFERENCED: &str = "referenced";

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

/// `POST /v1/workspaces/{workspace_id}/secrets` — store one under a free name.
///
/// A name this workspace already holds is refused rather than overwritten, and
/// the decision is Postgres's: two concurrent creates on one name resolve to
/// one 201 and one 409 with no window in which both saw the name free.
pub(crate) async fn store<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    body: Bytes,
) -> Result<Response, Refusal> {
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
            name: Cow::Borrowed(name.as_str()),
        }),
    )
        .into_response())
}

/// `GET /v1/workspaces/{workspace_id}/secrets` — the descriptors, never a value.
///
/// Answers from the projection columns alone. No envelope is opened, and the
/// half of the store this reaches holds no key with which to open one.
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
        .replace(&owned.workspace, &name, &secret, services.now())
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
pub(crate) async fn remove<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(SecretPath { name }): Path<SecretPath>,
) -> Result<Response, Refusal> {
    let name = SecretName::parse(&name).map_err(Refusal::at(EVENT_DELETE))?;

    match services.secrets().remove(&owned.workspace, &name).await {
        Ok(Deleted::Removed | Deleted::AlreadyAbsent) => Ok(StatusCode::NO_CONTENT.into_response()),
        // The one refusal whose sentence the call site knows better than the
        // error's own table does: `detail()` is a `&'static str` and the count
        // is not, so the counted form is rendered here — the same split
        // `Refusal::preconditioned` makes for a stale `ETag`.
        Err(refused) => Err(match refused.referenced_by() {
            Some(entries) => Refusal::conflict_detailed(
                EVENT_DELETE,
                referenced_detail(entries),
                STATE_REFERENCED,
            )(refused),
            None => Refusal::at(EVENT_DELETE)(refused),
        }),
    }
}

/// The sentence a still-referenced delete is refused with.
///
/// `secrets.zig`'s wording, plural included, because a dashboard shows this
/// string to the operator who has to go and remove those entries.
fn referenced_detail(entries: u32) -> String {
    let plural = if entries == 1 { "y" } else { "ies" };
    format!("Secret is referenced by {entries} model registry entr{plural}")
}

/// One stored projection, as the list emits it.
fn summary(held: &SecretSummary) -> afd_wire::secret::SecretSummary<'_> {
    afd_wire::secret::SecretSummary {
        name: Cow::Borrowed(&held.name),
        created_at: held.created_at_ms,
        kind: held.kind.as_str(),
        provider: held.provider.as_deref().map(Cow::Borrowed),
        base_url: held.base_url.as_deref().map(Cow::Borrowed),
    }
}

/// Reads a request body as `T`, refusing anything that is not an object.
///
/// Shared by both writing verbs so their two refusals are one sentence. An
/// EMPTY body is told apart from a malformed one: the fleet install can default
/// to `{}` because every field there is optional, and here there would be no
/// secret to store.
fn read_body<'b, T: serde::Deserialize<'b>>(body: &'b Bytes) -> Result<T, Refusal> {
    if body.is_empty() {
        return Err(Refusal::malformed(DETAIL_BODY_REQUIRED));
    }
    afd_core::json::object_from_slice::<T>(body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED_JSON))
}
