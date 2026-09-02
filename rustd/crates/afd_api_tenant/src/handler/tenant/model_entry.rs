//! `/v1/tenants/me/models` — the registry a tenant picks its provider from.
//!
//! Four verbs over one store. [`super::provider`] holds the selection; this
//! holds the list it is chosen from, and the two share a page in the dashboard.
//!
//! # The refusals are answered from values, not from failures
//!
//! Every way a client is told no arrives as an outcome variant the store
//! decided from a row it already held — a credential nobody stored, a pair the
//! tenant already has, an id that does not resolve, an entry that is the active
//! selection. So this crate keeps no error type, the shape
//! `RUST_ERROR_STANDARD` records for every plane crate, and each registry code
//! is chosen where the fact is known. Each match is exhaustive, so a variant
//! added to an outcome fails to compile until this says what a client is told.
//!
//! # The cursor is bound to the query that issued it
//!
//! A registry token carries the tenant and the page size it was minted under,
//! and a token naming either differently is refused as `UZ-LIBRARY-002` rather
//! than silently answered. Nothing is trusted FROM the cursor except the sort
//! boundary: the tenant a page reads is always the authenticated one.
//!
//! That is two distinct refusals on purpose. A token that will not decode is
//! `UZ-LIBRARY-001` — the client did not send something this endpoint issued.
//! One that decodes but names another tenant is `UZ-LIBRARY-002`, and folding
//! them would hide a cross-tenant replay attempt inside the same signal as a
//! truncated URL.
//!
//! # Input is refused before the tenant is resolved, and the Zig's order differs
//!
//! `hx.principal.tenant_id` is already on the Zig's request context, so checking
//! it first costs that daemon nothing. Here [`tenant_of`] is a READ, so the same
//! order would spend a pool connection to reject `?limit=0`. The refusals below
//! that need no tenant therefore run first.
//!
//! The observable difference is confined to one state: an authenticated
//! principal that resolves to NO tenant row, sending a malformed request. The
//! Zig answers 403 and this answers 400. It is declared rather than corrected
//! because correcting it would cost a datastore round trip on every malformed
//! request AND would make the input bounds unprovable at router tier — the
//! suite beside this file proves them with no Postgres precisely because they
//! are decided before one is touched.

use std::sync::Arc;

use afd_core::error_code;
use afd_core::paging::struct_cursor::StructCursor;
use afd_credential::provider::{Added, Removed, Retargeted};
use afd_wire::tenant_model_entry::{CreateModelEntryRequest, UpdateModelEntryRequest};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
use crate::services::{Services, TenantModelEntries as _};

// Three sentences the catalogue page already owns, imported rather than
// respelled (RULE UFS). The fourth — a query string this daemon cannot decode —
// has no path here: `RawQuery` hands over the raw text and `parameter` cannot
// fail, where the Zig's `req.query()` can. One fewer refusal, and the
// divergence is that a token with a stray `%` is simply not a cursor this
// endpoint issued.
// Two sentences the provider surface already owns, byte-identical here because
// the fact is the same one: a body this daemon cannot read, and a tenant whose
// bootstrap never produced a workspace (RULE UFS).
use super::provider::{DETAIL_MALFORMED_BODY, DETAIL_NO_PRIMARY_WORKSPACE};
use super::tenant_of;

/// The scoped events each verb's failures are logged under.
const EVENT_LIST: &str = "model_entries_list_failed";
const EVENT_CREATE: &str = "model_entry_create_failed";
const EVENT_UPDATE: &str = "model_entry_update_failed";
const EVENT_DELETE: &str = "model_entry_delete_failed";
const EVENT_TENANT: &str = "model_entry_tenant_unresolved";

/// The refusal a real cursor for a different walk earns.
///
/// Its own sentence rather than the catalogue's: that page binds a token to the
/// FILTERS it was issued under, this one binds it to the TENANT, and a client
/// told the wrong thing looks for the wrong mistake.
pub const DETAIL_CURSOR_MISMATCH: &str =
    "starting_after was issued for a different tenant or page size";

/// The refusal a path segment that is not an identifier earns.
pub const DETAIL_ENTRY_ID: &str = "id must be a valid UUIDv7";

/// The refusal a body naming no model earns.
pub const DETAIL_MODEL_ID_REQUIRED: &str = "model_id is required";

/// The refusal a model name past its bound earns.
pub const DETAIL_MODEL_ID_TOO_LONG: &str = "model_id must be at most 256 chars";

/// The refusal a body naming no credential earns.
///
/// Its own sentence rather than the provider surface's: that one names the
/// MODE the field is required under, and this verb has no modes.
pub const DETAIL_SECRET_REF_REQUIRED: &str = "secret_ref is required";

/// The refusal a credential the vault does not hold earns.
pub const DETAIL_SECRET_REF_UNKNOWN: &str =
    "secret_ref does not name a vault secret in this tenant's workspace";

/// The refusal a pair the tenant already registered earns.
pub const DETAIL_DUPLICATE_ENTRY: &str = "An entry with this model and secret already exists";

/// The refusal an id that resolves to nothing earns.
pub const DETAIL_ENTRY_NOT_FOUND: &str = "Model entry not found";

/// The refusal removing the tenant's current selection earns.
pub const DETAIL_DELETE_ACTIVE: &str =
    "This entry is the tenant's active selection; switch to another entry first";

/// The longest model identifier this surface accepts.
///
/// One rule and two call sites, so the bound cannot hold on the create and not
/// on the change — which is exactly how `model_id` ended up bounded on the
/// catalogue route and unbounded on this one.
pub(super) const MODEL_ID_MAX: usize = 256;

/// This page's cursor payload, in the Zig's fixed key order.
///
/// `tenant_uuid` and `limit` ride beside the sort key because a cursor is bound
/// to the walk that produced it. Field ORDER is the canonical key order — see
/// [`struct_cursor`] — so reordering this declaration invalidates every token
/// already in flight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cursor {
    /// The payload generation this cursor was issued under.
    v: u8,
    /// The boundary row's creation instant.
    created_at: i64,
    /// The boundary row's id, breaking ties within a millisecond.
    id: String,
    /// The tenant the walk was issued for.
    tenant_uuid: String,
    /// The page size the walk was issued under.
    limit: u32,
}

impl StructCursor for Cursor {
    fn generation(&self) -> u8 {
        self.v
    }
}

/// `GET /v1/tenants/me/models` — one page of the registry.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/tenants/me/models",
    tag = afd_http::openapi::tag::TENANT,
    operation_id = "list_tenant_model_entries",
    summary = "List the tenant's model registry",
    description = concat!(
        "One entry per configured model; two entries can share a ",
        "`secret_ref`. Each entry is joined to its secret's non-secret ",
        "metadata (provider, kind, base_url, has_key) — `api_key` is never ",
        "serialised. `active` is computed against the tenant's current ",
        "provider selection (`GET /v1/tenants/me/provider`). The response is ",
        "a bounded page ordered by `created_at` descending, then `id` ",
        "descending. Page forward by sending the response's `next_cursor` ",
        "back as `starting_after`; a `null` `next_cursor` is the last page. ",
        "Cursors are bound to the tenant and the `limit` that produced them, ",
        "so changing `limit` mid-pagination requires starting from the first ",
        "page again. ",
    ),
    params(
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let raw = query.unwrap_or_default();
    let limit = requested_limit(&raw)?;
    let tenant = tenant_of(
        &services,
        identity.person(),
        super::DETAIL_TENANT_REQUIRED,
        EVENT_TENANT,
    )
    .await?;
    let after = resume_from(&raw, &tenant, limit)?;

    let page = services
        .tenant_providers()
        .registry_page(&tenant, limit, after.as_ref())
        .await
        .map_err(Refusal::at(EVENT_LIST))?;

    Ok(Json(rendered(&page, &tenant, limit)).into_response())
}

/// `POST /v1/tenants/me/models` — register a model on a stored credential.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/tenants/me/models",
    tag = afd_http::openapi::tag::TENANT,
    operation_id = "create_tenant_model_entry",
    summary = "Register a model entry",
    description = concat!(
        "Creates one `(model_id, secret_ref)` row. `secret_ref` must already ",
        "name a vault secret in the tenant's primary workspace — store or ",
        "reuse one via `/v1/workspaces/{workspace_id}/secrets` first. Does ",
        "not activate the entry; activate via `PUT /v1/tenants/me/provider`. ",
    ),
    request_body = CreateModelEntryRequest,
    responses(
        (status = 201, description = afd_http::openapi::CREATED),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: CreateModelEntryRequest = afd_core::json::object_from_slice(&body)
        .map_err(|_shape| Refusal::malformed(DETAIL_MALFORMED_BODY))?;
    let model = bounded_model(&request.model_id)?;
    if request.secret_ref.is_empty() {
        return Err(Refusal::malformed(DETAIL_SECRET_REF_REQUIRED));
    }

    let tenant = tenant_of(
        &services,
        identity.person(),
        super::DETAIL_TENANT_REQUIRED,
        EVENT_TENANT,
    )
    .await?;

    let outcome = services
        .tenant_providers()
        .add_entry(&tenant, model, &request.secret_ref, services.now())
        .await
        .map_err(Refusal::at(EVENT_CREATE))?;

    match outcome {
        Added::Stored(entry) => Ok((StatusCode::CREATED, Json(stored(&entry))).into_response()),
        Added::CredentialMissing => Err(Refusal::coded(
            error_code::MODELS_SECRET_NOT_FOUND,
            DETAIL_SECRET_REF_UNKNOWN,
        )),
        Added::Duplicate => Err(Refusal::coded(
            error_code::MODELS_DUPLICATE_ENTRY,
            DETAIL_DUPLICATE_ENTRY,
        )),
        Added::NoWorkspace => Err(Refusal::coded(
            error_code::TENANT_NO_PRIMARY_WORKSPACE,
            DETAIL_NO_PRIMARY_WORKSPACE,
        )),
    }
}

/// `PATCH /v1/tenants/me/models/{id}` — point an entry at another model.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/tenants/me/models/{id}",
    tag = afd_http::openapi::tag::TENANT,
    operation_id = "update_tenant_model_entry",
    summary = "Change a model entry's model_id",
    description = concat!(
        "Model-only change; `secret_ref` is immutable on this endpoint — ",
        "create a new entry to point at a different secret. ",
    ),
    request_body = UpdateModelEntryRequest,
    params(
        afd_http::openapi::path::Id,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn update<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(entry_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let entry = parse_entry_id(&entry_id)?;
    let request: UpdateModelEntryRequest = afd_core::json::object_from_slice(&body)
        .map_err(|_shape| Refusal::malformed(DETAIL_MALFORMED_BODY))?;
    let model = bounded_model(&request.model_id)?;

    let tenant = tenant_of(
        &services,
        identity.person(),
        super::DETAIL_TENANT_REQUIRED,
        EVENT_TENANT,
    )
    .await?;

    let outcome = services
        .tenant_providers()
        .set_entry_model(&tenant, &entry, model, services.now())
        .await
        .map_err(Refusal::at(EVENT_UPDATE))?;

    match outcome {
        Retargeted::Stored(entry) => Ok(Json(stored(&entry)).into_response()),
        Retargeted::NotFound => Err(Refusal::coded(
            error_code::MODELS_ENTRY_NOT_FOUND,
            DETAIL_ENTRY_NOT_FOUND,
        )),
        Retargeted::Duplicate => Err(Refusal::coded(
            error_code::MODELS_DUPLICATE_ENTRY,
            DETAIL_DUPLICATE_ENTRY,
        )),
    }
}

/// `DELETE /v1/tenants/me/models/{id}` — remove an entry.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/tenants/me/models/{id}",
    tag = afd_http::openapi::tag::TENANT,
    operation_id = "delete_tenant_model_entry",
    summary = "Remove a model entry",
    description = concat!(
        "Idempotent — deleting an id that doesn't exist (already removed, or ",
        "never existed) still returns 204. The referenced vault secret is ",
        "never touched; sibling entries sharing it survive. ",
    ),
    params(
        afd_http::openapi::path::Id,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn remove<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(entry_id): Path<String>,
) -> Result<Response, Refusal> {
    let entry = parse_entry_id(&entry_id)?;
    let tenant = tenant_of(
        &services,
        identity.person(),
        super::DETAIL_TENANT_REQUIRED,
        EVENT_TENANT,
    )
    .await?;

    match services
        .tenant_providers()
        .remove_entry(&tenant, &entry)
        .await
        .map_err(Refusal::at(EVENT_DELETE))?
    {
        Removed::Done => Ok(StatusCode::NO_CONTENT.into_response()),
        Removed::Active => Err(Refusal::coded(
            error_code::MODELS_DELETE_ACTIVE,
            DETAIL_DELETE_ACTIVE,
        )),
    }
}

mod input;
mod render;

use self::input::{bounded_model, parse_entry_id, resume_from};
use self::render::{rendered, stored};
use crate::handler::paging::requested_limit;
