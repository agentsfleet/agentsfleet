//! The four verbs over a tenant's own `agt_t` credentials.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_core::paging::{Boundary as _, Page};
use afd_tenant::apikey::{
    ApiKeySort, Deactivation, Description, KeyName, KeyRow, Listing, MintRequest, Revealed, Revoked,
};
use afd_wire::tenant::{
    ApiKeySummary, MintApiKeyRequest, MintedApiKeyResponse, PageResponse, PatchApiKeyRequest,
    RevokedApiKeyResponse,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::{HeaderValue, StatusCode, header};

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
use crate::services::{Services, TenantKeys as _};

use super::{parameter, tenant_of};

/// The scoped events each verb's failures are logged under.
const EVENT_MINT: &str = "apikey_mint_failed";
const EVENT_LIST: &str = "apikey_list_failed";
const EVENT_REVOKE: &str = "apikey_revoke_failed";
const EVENT_DELETE: &str = "apikey_delete_failed";
const EVENT_TENANT: &str = "apikey_tenant_unresolved";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MINT_BODY: &str = "Malformed JSON body";

/// The refusal a patch body that is not `{"active": false}` earns.
const DETAIL_PATCH_BODY: &str = "PATCH body must be {\"active\": false}";

/// The refusal a path segment that is not an identifier earns.
const DETAIL_KEY_ID: &str = "id must be a valid UUIDv7";

/// The refusal a principal with no tenant to act for earns.
///
/// Reached by a bootstrap credential — one that authenticates without resolving
/// to a tenant row. It cannot manage keys because there is no tenant for the
/// key to belong to, which is a fact about the credential rather than about the
/// caller's capabilities.
const DETAIL_NO_TENANT: &str =
    "Tenant context required; bootstrap principals cannot manage tenant API keys";

/// `POST /v1/api-keys` — mint one, revealing it exactly once.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/api-keys",
    tag = afd_http::openapi::tag::API_KEYS,
    operation_id = "create_api_key",
    summary = "Mint a tenant API key",
    description = concat!(
        "Creates a tenant admin key with the `agt_t` prefix. The raw key is ",
        "returned once. Only its SHA-256 hash is saved. ",
    ),
    request_body = MintApiKeyRequest,
    params(
    ),
    responses(
        (status = 201, description = afd_http::openapi::CREATED, body = MintedApiKeyResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn mint<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    body: Bytes,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let request = afd_core::json::object_from_slice::<MintApiKeyRequest<'_>>(&body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MINT_BODY))?;
    let (name, description) = KeyName::parse(&request.key_name)
        .and_then(|name| {
            Description::parse(request.description.as_deref())
                .map(|description| (name, description))
        })
        .map_err(Refusal::at(EVENT_MINT))?;

    let tenant = tenant_of(&services, person, DETAIL_NO_TENANT, EVENT_TENANT).await?;
    let mint = MintRequest {
        tenant: &tenant,
        name,
        description,
        created_by: person.subject().as_str(),
    };

    let revealed = services
        .api_keys()
        .mint(&mint, services.now())
        .await
        .map_err(Refusal::at(EVENT_MINT))?;
    Ok(revealed_response(&revealed))
}

/// `GET /v1/api-keys` — the tenant's keys, as metadata.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/api-keys",
    tag = afd_http::openapi::tag::API_KEYS,
    operation_id = "list_api_keys",
    summary = "List tenant API keys",
    description = concat!(
        "Returns metadata for every tenant API key visible to the caller, ",
        "under Stripe-style keyset pagination. The cursor carries the ",
        "boundary sort value, so every allowlisted sort pages without loss. ",
        "The retired page and page_size parameters are refused. Results are ",
        "always scoped to the caller's tenant; `key_hash` is never returned. ",
    ),
    params(
        ("starting_after" = Option<String>, Query, description = "An opaque cursor from a previous page's `next_cursor`, issued under the same sort."),
        ("limit" = Option<String>, Query),
        ("sort" = Option<String>, Query),
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = PageResponse<ApiKeySummary>),
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
    let person = identity.person();
    let query = query.unwrap_or_default();
    let page = Page::<ApiKeySort>::parse(|name| parameter(&query, name))
        .map_err(|refusal| Refusal::malformed(refusal.detail()))?;

    let tenant = tenant_of(&services, person, DETAIL_NO_TENANT, EVENT_TENANT).await?;

    let listing = services
        .api_keys()
        .list(&tenant, &page)
        .await
        .map_err(Refusal::at(EVENT_LIST))?;
    Ok(Json(page_response(&listing, &page)).into_response())
}

/// `PATCH /v1/api-keys/{id}` — revoke one.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/api-keys/{id}",
    tag = afd_http::openapi::tag::API_KEYS,
    operation_id = "revoke_api_key",
    summary = "Revoke a tenant API key",
    description = concat!(
        "Partial lifecycle update. Body must be `{\"active\": false}` — re- ",
        "activation is not supported; mint a new key instead. Sets ",
        "`revoked_at` and `updated_at`. ",
    ),
    request_body = PatchApiKeyRequest,
    params(
        afd_http::openapi::path::Id,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = RevokedApiKeyResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn revoke<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(key_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let key = Uuid7::parse(&key_id).map_err(|_unparseable| Refusal::malformed(DETAIL_KEY_ID))?;
    let request = afd_core::json::object_from_slice::<PatchApiKeyRequest>(&body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_PATCH_BODY))?;
    // The intent is PARSED, not checked: `revoke` takes a `Deactivation`, so
    // there is no path to it that skipped this refusal.
    let intent = Deactivation::parse(request.active).map_err(Refusal::at(EVENT_REVOKE))?;

    let tenant = tenant_of(&services, person, DETAIL_NO_TENANT, EVENT_TENANT).await?;

    let revoked = services
        .api_keys()
        .revoke(&tenant, &key, intent, services.now())
        .await
        .map_err(Refusal::at(EVENT_REVOKE))?;
    Ok(Json(revoked_response(&revoked)).into_response())
}

/// `DELETE /v1/api-keys/{id}` — remove one that is already revoked.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/api-keys/{id}",
    tag = afd_http::openapi::tag::API_KEYS,
    operation_id = "delete_api_key",
    summary = "Delete a revoked tenant API key",
    description = concat!(
        "Deletes a revoked tenant API key. An active key returns 409 `UZ- ",
        "APIKEY-008`. ",
    ),
    params(
        afd_http::openapi::path::Id,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn delete<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(key_id): Path<String>,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let key = Uuid7::parse(&key_id).map_err(|_unparseable| Refusal::malformed(DETAIL_KEY_ID))?;
    let tenant = tenant_of(&services, person, DETAIL_NO_TENANT, EVENT_TENANT).await?;

    services
        .api_keys()
        .delete(&tenant, &key)
        .await
        .map_err(Refusal::at(EVENT_DELETE))?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// The mint reply, with the header that keeps it out of a cache.
///
/// `no-store` and not merely `no-cache`: the body carries a credential in
/// plaintext exactly once, and an intermediary holding a copy is a copy nobody
/// can revoke. This is the port of `hx.okSensitive`.
fn revealed_response(revealed: &Revealed) -> Response {
    (
        StatusCode::CREATED,
        [(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, max-age=0"),
        )],
        Json(MintedApiKeyResponse {
            id: Cow::Borrowed(revealed.id.as_str()),
            key_name: Cow::Borrowed(&revealed.name),
            key: Cow::Borrowed(revealed.credential.expose()),
            created_at: revealed.created_at_ms,
        }),
    )
        .into_response()
}

/// The revoke reply.
fn revoked_response(revoked: &Revoked) -> RevokedApiKeyResponse<'_> {
    RevokedApiKeyResponse {
        id: Cow::Borrowed(revoked.id.as_str()),
        // Always false, and stated rather than read back: the statement only
        // answers `changed = TRUE` for a row it moved to inactive.
        active: false,
        revoked_at: revoked.revoked_at_ms,
    }
}

/// One page, and the cursor that continues it.
///
/// A cursor is emitted only when the page is FULL — the total minus what has
/// been seen would disagree with what the cursor can actually reach after a
/// row is deleted mid-walk, and a client would ask for a page that comes back
/// empty.
fn page_response<'rows>(
    listing: &'rows Listing,
    page: &Page<ApiKeySort>,
) -> PageResponse<'rows, ApiKeySummary<'rows>> {
    // `try_from` rather than a cast: a page longer than `u32` cannot happen —
    // the limit is bounded at a hundred — and a cast would say so by silently
    // truncating rather than by being unreachable.
    let full = u32::try_from(listing.keys.len()).is_ok_and(|count| count == page.limit);
    let next_cursor = full
        .then(|| listing.keys.last())
        .flatten()
        .map(|last| Cow::Owned(last.cursor(page.sort).to_string()));
    PageResponse {
        items: listing.keys.iter().map(summary).collect(),
        total: listing.total,
        next_cursor,
    }
}

/// One row as the wire shows it.
fn summary(key: &KeyRow) -> ApiKeySummary<'_> {
    ApiKeySummary {
        id: Cow::Borrowed(&key.id),
        key_name: Cow::Borrowed(&key.name),
        active: key.active,
        created_at: key.created_at_ms,
        last_used_at: key.last_used_at_ms,
        revoked_at: key.revoked_at_ms,
    }
}
