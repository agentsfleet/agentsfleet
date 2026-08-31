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
use afd_core::id::Uuid7;
use afd_core::paging::struct_cursor::{self, StructCursor};
use afd_core::paging::{DEFAULT_LIMIT, MAX_LIMIT, QUERY_LIMIT, QUERY_STARTING_AFTER};
use afd_credential::provider::{
    Added, Boundary, PricedDefault, RegistryPage, RegistryRow, Removed, Retargeted,
};
use afd_wire::tenant_model_entry::{
    CreateModelEntryRequest, ModelEntriesResponse, ModelEntryRow, PlatformDefaultRow,
    StoredModelEntry, UpdateModelEntryRequest,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::auth::PersonIdentity;
use crate::handler::{Refusal, parameter};
use crate::services::{Services, TenantModelEntries as _};

// Three sentences the catalogue page already owns, imported rather than
// respelled (RULE UFS). The fourth — a query string this daemon cannot decode —
// has no path here: `RawQuery` hands over the raw text and `parameter` cannot
// fail, where the Zig's `req.query()` can. One fewer refusal, and the
// divergence is that a token with a stray `%` is simply not a cursor this
// endpoint issued.
use super::models::{DETAIL_CATALOGUE_LIMIT, DETAIL_CURSOR_MALFORMED};
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
const MODEL_ID_MAX: usize = 256;

/// This page's cursor payload, in the Zig's fixed key order.
///
/// `tenant_uuid` and `limit` ride beside the sort key because a cursor is bound
/// to the walk that produced it. Field ORDER is the canonical key order — see
/// [`struct_cursor`] — so reordering this declaration invalidates every token
/// already in flight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
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

/// The page size this request asked for, already bounded.
fn requested_limit(raw: &str) -> Result<u32, Refusal> {
    let Some(asked) = parameter(raw, QUERY_LIMIT) else {
        return Ok(DEFAULT_LIMIT);
    };
    asked
        .parse::<u32>()
        .ok()
        .filter(|limit| (1..=MAX_LIMIT).contains(limit))
        .ok_or_else(|| {
            Refusal::coded(
                error_code::LIBRARY_INPUT_OUT_OF_BOUNDS,
                DETAIL_CATALOGUE_LIMIT,
            )
        })
}

/// The boundary this request resumes from, or nothing for the first page.
///
/// The identity check is here and not in the store: only this function knows
/// which tenant authenticated and which limit was asked for, which is the whole
/// reason the seam takes a [`Boundary`] rather than a token.
fn resume_from(raw: &str, tenant: &Uuid7, limit: u32) -> Result<Option<Boundary>, Refusal> {
    let Some(token) = parameter(raw, QUERY_STARTING_AFTER).filter(|token| !token.is_empty()) else {
        return Ok(None);
    };
    let cursor: Cursor = struct_cursor::parse(token).map_err(|_foreign| {
        Refusal::coded(
            error_code::LIBRARY_CURSOR_MALFORMED,
            DETAIL_CURSOR_MALFORMED,
        )
    })?;
    if cursor.tenant_uuid != tenant.as_str() || cursor.limit != limit {
        return Err(Refusal::coded(
            error_code::LIBRARY_CURSOR_MISMATCH,
            DETAIL_CURSOR_MISMATCH,
        ));
    }
    // The id is the only field taken from the token besides the instant, and it
    // is re-parsed rather than trusted: a `::uuid` cast is not the place to
    // discover that a client sent something else.
    let id = Uuid7::parse(&cursor.id).map_err(|_not_an_identifier| {
        Refusal::coded(
            error_code::LIBRARY_CURSOR_MALFORMED,
            DETAIL_CURSOR_MALFORMED,
        )
    })?;
    Ok(Some(Boundary {
        created_at_ms: cursor.created_at,
        id,
    }))
}

/// The entry a path segment names.
fn parse_entry_id(raw: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(raw).map_err(|_not_an_identifier| Refusal::malformed(DETAIL_ENTRY_ID))
}

/// A model name within its bound, or the refusal it earns.
///
/// Blank and oversized are different sentences because the repairs differ, and
/// the bound is checked here rather than at the store: a name past it is a
/// malformed REQUEST, and the column would take it.
fn bounded_model(model_id: &str) -> Result<&str, Refusal> {
    if model_id.is_empty() {
        return Err(Refusal::malformed(DETAIL_MODEL_ID_REQUIRED));
    }
    if model_id.len() > MODEL_ID_MAX {
        return Err(Refusal::malformed(DETAIL_MODEL_ID_TOO_LONG));
    }
    Ok(model_id)
}

/// The written row, rendered.
fn stored(entry: &afd_credential::provider::Entry) -> StoredModelEntry<'_> {
    StoredModelEntry {
        id: entry.id.as_str(),
        model_id: &entry.model_id,
        secret_ref: &entry.secret_ref,
        created_at: entry.created_at_ms,
    }
}

/// The page, rendered.
fn rendered<'p>(page: &'p RegistryPage, tenant: &Uuid7, limit: u32) -> ModelEntriesResponse<'p> {
    ModelEntriesResponse {
        models: page.rows.iter().map(row).collect(),
        // Always null: counting a keyset page costs the scan this pagination
        // exists to avoid, and the key stays present rather than vanishing.
        total: None,
        next_cursor: page.next.as_ref().map(|boundary| {
            struct_cursor::render(&Cursor {
                v: struct_cursor::VERSION,
                created_at: boundary.created_at_ms,
                id: boundary.id.as_str().to_owned(),
                tenant_uuid: tenant.as_str().to_owned(),
                limit,
            })
        }),
        platform_default_available: page.platform_default.is_some(),
        platform_default: page.platform_default.as_ref().map(default_row),
    }
}

/// One row, rendered.
///
/// A credential the vault could not describe degrades to an opaque secret with
/// no key and sheds its descriptors — the same shape the workspace secret list
/// gives a row it cannot label, and the reason a dangling reference lists at
/// all instead of failing the page.
fn row(entry: &RegistryRow) -> ModelEntryRow<'_> {
    let rate = entry.rate.as_ref();
    ModelEntryRow {
        id: entry.entry.id.as_str(),
        model_id: &entry.entry.model_id,
        secret_ref: &entry.entry.secret_ref,
        provider: entry
            .credential
            .as_ref()
            .and_then(|held| held.provider.as_deref()),
        kind: entry
            .credential
            .as_ref()
            .map_or(afd_vault::Kind::CustomSecret.as_str(), |held| {
                held.kind.as_str()
            }),
        base_url: entry
            .credential
            .as_ref()
            .and_then(|held| held.base_url.as_deref()),
        has_key: entry.credential.as_ref().is_some_and(|held| held.has_key),
        context_cap_tokens: rate.map(|rate| rate.context_cap_tokens),
        input_nanos_per_mtok: rate.map(|rate| rate.input_nanos_per_mtok),
        cached_input_nanos_per_mtok: rate.map(|rate| rate.cached_input_nanos_per_mtok),
        output_nanos_per_mtok: rate.map(|rate| rate.output_nanos_per_mtok),
        active: entry.active,
        created_at: entry.entry.created_at_ms,
    }
}

/// The platform default, rendered.
fn default_row(priced: &PricedDefault) -> PlatformDefaultRow<'_> {
    let rate = priced.rate.as_ref();
    PlatformDefaultRow {
        provider: &priced.default.provider,
        model: &priced.default.model,
        context_cap_tokens: priced.default.context_cap_tokens,
        input_nanos_per_mtok: rate.map(|rate| rate.input_nanos_per_mtok),
        cached_input_nanos_per_mtok: rate.map(|rate| rate.cached_input_nanos_per_mtok),
        output_nanos_per_mtok: rate.map(|rate| rate.output_nanos_per_mtok),
    }
}
