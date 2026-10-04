//! Platform Fleet-library catalogue HTTP adapters.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::error_code;
use afd_library::{DeleteLibrary, LibraryItem, MAX_SKILL_NAME_LEN, PatchLibrary};
use afd_wire::admin::{AdminLibrariesResponse, AdminLibraryItem, AdminLibraryRequirements};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use const_format::concatcp;
use garde::Validate as _;
use http::{HeaderMap, StatusCode, header};

use crate::auth::PersonIdentity;
use crate::envelope::ProblemResponse;
use crate::handler::{refuse, reject};
use crate::request_id::RequestId;
use crate::services::Services;

use super::libraries_request::patch_request;

const DETAIL_ID_BOUNDS: &str = concatcp!("catalog id must be 1-", MAX_SKILL_NAME_LEN, " bytes");
const DETAIL_NOT_FOUND: &str = "No fleet library entry has that catalog id";
const DETAIL_NO_BUNDLE: &str =
    "This entry has no bundle. Fetch it from its repository first, then publish.";
const DETAIL_STALE: &str = "This catalog entry changed since you loaded it. Refresh to see the latest, then re-apply your edit.";
const DETAIL_DELETE_PUBLISHED: &str =
    "This fleet is published. Unpublish it first, then delete it.";

/// A catalogue entry addressed by its catalog id, as the published document
/// describes the path: the bundle's name, within the bound the handler holds
/// it to, and never a UUID.
#[cfg(feature = "openapi")]
#[derive(Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Path)]
struct CatalogIdPath {
    /// The entry's catalog id: the bundle's name, 1 to 64 bytes.
    #[param(min_length = 1, max_length = 64)]
    #[expect(
        dead_code,
        reason = "read by the OpenAPI derive alone, as every path parameter is"
    )]
    id: String,
}

// The published bound and the enforced one are the same number, pinned here
// because utoipa takes a literal where the handler takes the constant.
#[cfg(feature = "openapi")]
const _: () = assert!(MAX_SKILL_NAME_LEN == 64);

/// Lists every platform row, including drafts and entries with no bundle.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/admin/fleet-libraries",
    tag = afd_http::openapi::tag::FLEET_LIBRARY,
    operation_id = "list_platform_fleet_library",
    summary = "List the platform Fleet library catalog",
    description = concat!(
        "Lists every entry in the global platform catalog. Published, draft, ",
        "and entries whose bundle was never fetched all appear. Unlike the ",
        "workspace gallery, this operator view hides nothing: it shows what ",
        "is live and what still needs work. Requires the ",
        "`platform-library:write` scope. Metadata only — never bundle markdown, a ",
        "support-file body, or an object-store key. Each row carries an ",
        "`etag` that an editor can send as `If-Match` on PATCH. ",
    ),
    params(
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = AdminLibrariesResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn list<D: Services>(State(services): State<Arc<D>>) -> Response {
    match services.libraries().list().await {
        Ok(entries) => Json(AdminLibrariesResponse {
            entries: entries.iter().map(item).collect(),
        })
        .into_response(),
        Err(error) => refuse(&error, "admin_libraries_list_failed"),
    }
}

/// Curates, publishes, or withdraws one platform row.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/admin/fleet-libraries/{id}",
    tag = afd_http::openapi::tag::FLEET_LIBRARY,
    operation_id = "update_platform_fleet_library",
    summary = "Curate, publish, or unpublish a platform Fleet library entry",
    description = concat!(
        "Partial update. `description` and `required_credentials_reasons` are ",
        "the two fields no bundle can supply, so they are operator-owned: a ",
        "later bundle refetch never overwrites them. `published` moves the ",
        "entry between `draft` (stored, invisible to every tenant) and ",
        "`public` (live in every workspace gallery and installable). ",
        "Publishing an entry whose bundle was never fetched is refused — a ",
        "published entry always has something to install. Requires the ",
        "`platform-library:write` scope. Send `If-Match` with the row's ",
        "`etag` to reject stale edits before they can repoint the source or ",
        "unpublish the entry. Omitting the header preserves last-write-wins ",
        "behavior. ",
    ),
    request_body = afd_wire::admin::AdminLibraryPatch,
    params(
        CatalogIdPath,
        ("If-Match" = Option<String>, Header, description = "Optional catalog row version from the list response. Stale values return 412 with the current `etag`."),
    ),
    responses(
        (status = 200, description = "The entry as it now reads, under its new entity tag", body = AdminLibraryItem),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 412, description = afd_http::openapi::PRECONDITION_FAILED),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn patch<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(refusal) = refused_id(&id) {
        return refusal;
    }
    let patch = match patch_request(&body) {
        Ok(patch) => patch,
        Err((code, detail)) => return reject(code, detail),
    };
    let if_match = headers
        .get(header::IF_MATCH)
        .map(|value| value.to_str().unwrap_or_default());
    match services
        .libraries()
        .patch(&id, &patch, if_match, services.now())
        .await
    {
        Ok(PatchLibrary::Updated(entry)) => updated(&identity, &id, &entry),
        Ok(PatchLibrary::NotFound) => reject(error_code::CATALOG_NOT_FOUND, DETAIL_NOT_FOUND),
        Ok(PatchLibrary::PublishWithoutBundle) => ProblemResponse::conflict(
            error_code::CATALOG_PUBLISH_WITHOUT_BUNDLE,
            DETAIL_NO_BUNDLE,
            RequestId::mint(),
            "no_bundle",
        )
        .into_response(),
        Ok(PatchLibrary::Stale { etag }) => ProblemResponse::precondition_failed(
            error_code::CATALOG_ROW_STALE,
            DETAIL_STALE,
            RequestId::mint(),
            etag,
        )
        .into_response(),
        Err(error) => refuse(&error, "admin_library_patch_failed"),
    }
}

fn updated(identity: &PersonIdentity, id: &str, entry: &LibraryItem) -> Response {
    let actor_id = identity.subject();
    tracing::info!(actor_id, library_id = id, event = "admin_library_updated",);
    ([(header::ETAG, entry.etag().to_owned())], Json(item(entry))).into_response()
}

/// Deletes one draft; public entries must be withdrawn first.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/admin/fleet-libraries/{id}",
    tag = afd_http::openapi::tag::FLEET_LIBRARY,
    operation_id = "delete_platform_fleet_library",
    summary = "Delete an unpublished platform Fleet library entry",
    description = concat!(
        "Removes a catalog entry. Only an entry that is NOT published may be ",
        "deleted. A live fleet is never taken away from the tenants who can ",
        "install it, so unpublish it first. Workspaces that already installed ",
        "the fleet are unaffected: an install snapshots the bundle, so it ",
        "keeps running. Requires the `platform-library:write` scope. ",
    ),
    params(
        CatalogIdPath,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn delete<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(id): Path<String>,
) -> Response {
    if let Some(refusal) = refused_id(&id) {
        return refusal;
    }
    match services.libraries().delete(&id).await {
        Ok(DeleteLibrary::Deleted) => {
            tracing::info!(
                actor_id = identity.subject(),
                library_id = id,
                event = "admin_library_deleted",
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(DeleteLibrary::NotFound) => reject(error_code::CATALOG_NOT_FOUND, DETAIL_NOT_FOUND),
        Ok(DeleteLibrary::Published) => ProblemResponse::conflict(
            error_code::CATALOG_DELETE_PUBLISHED,
            DETAIL_DELETE_PUBLISHED,
            RequestId::mint(),
            "public",
        )
        .into_response(),
        Err(error) => refuse(&error, "admin_library_delete_failed"),
    }
}

/// The catalogue id a `PATCH` or `DELETE` path names.
///
/// Bounded by the skill-name bound the platform catalogue is keyed by: an id
/// longer than any bundle name can address no row, so the store is never
/// asked for one.
#[derive(Debug, garde::Validate)]
struct CatalogId<'a> {
    #[garde(length(bytes, min = 1, max = MAX_SKILL_NAME_LEN), custom(afd_validate::nul_free))]
    id: &'a str,
}

/// The refusal a path's catalogue id earns, or `None` when it can name a row.
fn refused_id(id: &str) -> Option<Response> {
    CatalogId { id }
        .validate()
        .err()
        .map(|_report| reject(error_code::INVALID_REQUEST, DETAIL_ID_BOUNDS))
}

fn item(entry: &LibraryItem) -> AdminLibraryItem<'static> {
    let requirements = entry.requirements();
    AdminLibraryItem {
        id: Cow::Owned(entry.id().to_owned()),
        name: Cow::Owned(entry.name().to_owned()),
        description: Cow::Owned(entry.description().to_owned()),
        source_repo: Cow::Owned(entry.source_repo().to_owned()),
        source_ref: Cow::Owned(entry.source_ref().to_owned()),
        visibility: Cow::Owned(entry.visibility().to_owned()),
        content_hash: entry.content_hash().map(|hash| Cow::Owned(hash.to_owned())),
        requirements: AdminLibraryRequirements {
            credentials: requirements
                .credentials()
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect(),
            tools: requirements
                .tools()
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect(),
            network_hosts: requirements
                .network_hosts()
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect(),
            trigger_present: requirements.trigger_present(),
        },
        required_credentials_reasons: entry.required_credentials_reasons().clone(),
        updated_at: entry.updated_at().as_millis(),
        etag: Cow::Owned(entry.etag().to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use afd_library::MAX_SKILL_NAME_LEN;

    use super::{DETAIL_ID_BOUNDS, refused_id};

    /// A catalogue id is refused one byte past the name bound and taken at
    /// it, and the sentence names the bound it enforces.
    #[test]
    fn a_catalog_id_is_bounded_by_the_name_it_is_keyed_by() {
        assert!(refused_id("").is_some());
        assert!(refused_id(&"n".repeat(MAX_SKILL_NAME_LEN)).is_none());
        assert!(refused_id(&"n".repeat(MAX_SKILL_NAME_LEN + 1)).is_some());
        assert!(
            refused_id("a\u{0}b").is_some(),
            "a NUL is refused at the edge, as every other bound here refuses it"
        );
        assert!(DETAIL_ID_BOUNDS.contains(&MAX_SKILL_NAME_LEN.to_string()));
    }
}
