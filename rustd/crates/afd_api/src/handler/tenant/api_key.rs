//! The four verbs over a tenant's own `agt_t` credentials.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_core::paging::{Cursor, Page};
use afd_fleet::apikey::{
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
use crate::handler::{malformed, refuse};
use crate::services::{Services, TenantKeys as _, WorkspaceOwnership as _};

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
pub(crate) async fn mint<D: Services>(
    State(services): State<Arc<D>>,
    PersonIdentity(person): PersonIdentity,
    body: Bytes,
) -> Response {
    let Ok(request) = afd_core::json::object_from_slice::<MintApiKeyRequest<'_>>(&body) else {
        return malformed(DETAIL_MINT_BODY);
    };
    let parsed = KeyName::parse(&request.key_name).and_then(|name| {
        Description::parse(request.description.as_deref()).map(|description| (name, description))
    });
    let (name, description) = match parsed {
        Ok(fields) => fields,
        Err(error) => return refuse(&error, EVENT_MINT),
    };

    let tenant = match tenant_of(&services, &person).await {
        Ok(tenant) => tenant,
        Err(response) => return *response,
    };
    let mint = MintRequest {
        tenant: &tenant,
        name,
        description,
        created_by: person.subject().as_str(),
    };

    match services.api_keys().mint(&mint, services.now()).await {
        Ok(revealed) => revealed_response(&revealed),
        Err(error) => refuse(&error, EVENT_MINT),
    }
}

/// `GET /v1/api-keys` — the tenant's keys, as metadata.
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    PersonIdentity(person): PersonIdentity,
    RawQuery(query): RawQuery,
) -> Response {
    let query = query.unwrap_or_default();
    let page = match Page::<ApiKeySort>::parse(|name| parameter(&query, name)) {
        Ok(page) => page,
        Err(refusal) => return malformed(refusal.detail()),
    };

    let tenant = match tenant_of(&services, &person).await {
        Ok(tenant) => tenant,
        Err(response) => return *response,
    };

    match services.api_keys().list(&tenant, &page).await {
        Ok(listing) => Json(page_response(&listing, page.limit)).into_response(),
        Err(error) => refuse(&error, EVENT_LIST),
    }
}

/// `PATCH /v1/api-keys/{id}` — revoke one.
pub(crate) async fn revoke<D: Services>(
    State(services): State<Arc<D>>,
    PersonIdentity(person): PersonIdentity,
    Path(key_id): Path<String>,
    body: Bytes,
) -> Response {
    let Ok(key) = Uuid7::parse(&key_id) else {
        return malformed(DETAIL_KEY_ID);
    };
    let Ok(request) = afd_core::json::object_from_slice::<PatchApiKeyRequest>(&body) else {
        return malformed(DETAIL_PATCH_BODY);
    };
    // The intent is PARSED, not checked: `revoke` takes a `Deactivation`, so
    // there is no path to it that skipped this refusal.
    let intent = match Deactivation::parse(request.active) {
        Ok(intent) => intent,
        Err(error) => return refuse(&error, EVENT_REVOKE),
    };

    let tenant = match tenant_of(&services, &person).await {
        Ok(tenant) => tenant,
        Err(response) => return *response,
    };

    match services
        .api_keys()
        .revoke(&tenant, &key, intent, services.now())
        .await
    {
        Ok(revoked) => Json(revoked_response(&revoked)).into_response(),
        Err(error) => refuse(&error, EVENT_REVOKE),
    }
}

/// `DELETE /v1/api-keys/{id}` — remove one that is already revoked.
pub(crate) async fn delete<D: Services>(
    State(services): State<Arc<D>>,
    PersonIdentity(person): PersonIdentity,
    Path(key_id): Path<String>,
) -> Response {
    let Ok(key) = Uuid7::parse(&key_id) else {
        return malformed(DETAIL_KEY_ID);
    };
    let tenant = match tenant_of(&services, &person).await {
        Ok(tenant) => tenant,
        Err(response) => return *response,
    };

    match services.api_keys().delete(&tenant, &key).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => refuse(&error, EVENT_DELETE),
    }
}

/// Which tenant this principal acts for, or the refusal.
///
/// The tenant plane's routes carry no workspace, so there is no ownership layer
/// in front of them and this is the boundary instead: every statement below
/// filters on what this returns, and a principal that resolves to no tenant
/// cannot reach a row at all.
async fn tenant_of<D: Services>(
    services: &Arc<D>,
    person: &afd_auth::principal::Person,
) -> Result<Uuid7, Box<Response>> {
    let principal = afd_auth::principal::Principal::Person(person.clone());
    match services.workspaces().tenant_of(&principal).await {
        Ok(Some(tenant)) => Ok(tenant),
        // Authenticated, and resolving to no tenant row. A 403 rather than a
        // 401: re-authenticating cannot produce a tenant this credential does
        // not have.
        //
        // Boxed, like the error arm below it: an `axum::Response` is over a
        // hundred bytes, and an unboxed one in the `Err` position makes every
        // caller's `Result` that size for a value that is discarded on the
        // path that matters (`clippy::result_large_err`).
        Ok(None) => Err(Box::new(
            crate::envelope::ProblemResponse::new(
                afd_core::error_code::AUTH_FORBIDDEN,
                DETAIL_NO_TENANT,
                crate::request_id::RequestId::mint(),
            )
            .into_response(),
        )),
        Err(error) => Err(Box::new(refuse(&error, EVENT_TENANT))),
    }
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
/// `has_more` is the page being FULL, not the total minus what has been seen: a
/// row deleted mid-walk would make the second disagree with what the cursor can
/// actually reach, and a client would ask for a page that comes back empty.
fn page_response(listing: &Listing, limit: u32) -> PageResponse<'_, ApiKeySummary<'_>> {
    // `try_from` rather than a cast: a page longer than `u32` cannot happen —
    // the limit is bounded at a hundred — and a cast would say so by silently
    // truncating rather than by being unreachable.
    let full = u32::try_from(listing.keys.len()).is_ok_and(|count| count == limit);
    let next_cursor = full
        .then(|| listing.keys.last())
        .flatten()
        .map(|last| Cow::Owned(cursor_for(last).to_string()));
    PageResponse {
        data: listing.keys.iter().map(summary).collect(),
        has_more: next_cursor.is_some(),
        next_cursor,
        total: listing.total,
    }
}

/// The cursor a client resumes from after `last`.
///
/// Always the creation-time form. The name-ordered walks carry a text boundary
/// and this would be the wrong one for them — which the paging layer REFUSES
/// rather than silently accepts, so a name-sorted page is currently a single
/// page. That is a gap, and it is a loud one: `test_list_keyset_pagination`
/// covers the timestamp orderings, and continuing a name walk needs the sort
/// threaded through here, which lands with the tenant-plane list handlers that
/// share this helper.
fn cursor_for(last: &KeyRow) -> Cursor {
    Cursor::Timestamp {
        at_ms: last.created_at_ms,
        id: last.id.clone(),
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

/// One query-string parameter, by name.
///
/// A hand-rolled scan rather than a query-string crate, because that is the
/// whole of what these handlers need from a query string and a crate for it
/// would be a dependency to justify. Percent-decoding is deliberately absent:
/// every value these parameters take — a limit, a sort spelling, a cursor —
/// is drawn from an alphabet that survives a URL unescaped, and a decoder here
/// would be a second place for a `+` to become a space.
fn parameter<'q>(query: &'q str, name: &str) -> Option<&'q str> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then_some(value)
    })
}
