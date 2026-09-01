//! Platform Fleet-library catalogue HTTP adapters.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::error_code;
use afd_library::{
    DeleteLibrary, LibraryItem, LibraryPatch, PatchLibrary, Repository, valid_revision,
};
use afd_wire::admin::{
    AdminLibrariesResponse, AdminLibraryItem, AdminLibraryPatch, AdminLibraryRequirements,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::{HeaderMap, StatusCode, header};

use crate::auth::PersonIdentity;
use crate::envelope::ProblemResponse;
use crate::handler::{refuse, reject};
use crate::request_id::RequestId;
use crate::services::Services;

const DETAIL_ID_REQUIRED: &str = "A catalog id is required";
const DETAIL_BODY_REQUIRED: &str = "A request body is required";
const DETAIL_MALFORMED_JSON: &str = "The request body is not valid JSON";
const DETAIL_NOT_FOUND: &str = "No fleet library entry has that catalog id";
const DETAIL_NAME_INVALID: &str = "A name is required, and must be at most 200 characters";
const DETAIL_REPO_INVALID: &str =
    "A repository must be owner/repo, using letters, digits, '.', '-' or '_'";
const DETAIL_REF_INVALID: &str =
    "A ref must be a branch or tag name, using letters, digits, '.', '-' or '_'";
const DETAIL_REASONS_INVALID: &str =
    "required_credentials_reasons must be an object mapping credential names to strings";
const DETAIL_REASONS_TOO_MANY: &str =
    "required_credentials_reasons carries more entries than a fleet may declare credentials";
const DETAIL_REASON_TOO_LONG: &str =
    "A credential name, or its reason copy, is longer than the install gate accepts";
const DETAIL_NO_BUNDLE: &str =
    "This entry has no bundle. Fetch it from its repository first, then publish.";
const DETAIL_STALE: &str = "This catalog entry changed since you loaded it. Refresh to see the latest, then re-apply your edit.";
const DETAIL_DELETE_PUBLISHED: &str =
    "This fleet is published. Unpublish it first, then delete it.";

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
        "is live and what still needs work. Requires the `platform- ",
        "library:write` scope. Metadata only — never bundle markdown, a ",
        "support-file body, or an object-store key. Each row carries an ",
        "`etag` that an editor can send as `If-Match` on PATCH. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = AdminLibrariesResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
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
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 412, description = afd_http::openapi::PRECONDITION_FAILED),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn patch<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if id.is_empty() {
        return reject(error_code::INVALID_REQUEST, DETAIL_ID_REQUIRED);
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
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
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
    Path(id): Path<String>,
) -> Response {
    if id.is_empty() {
        return reject(error_code::INVALID_REQUEST, DETAIL_ID_REQUIRED);
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

fn patch_request(body: &[u8]) -> Result<LibraryPatch, (error_code::ErrorCode, &'static str)> {
    if body.is_empty() {
        return Err((error_code::INVALID_REQUEST, DETAIL_BODY_REQUIRED));
    }
    let request = afd_core::json::object_from_slice::<AdminLibraryPatch<'_>>(body)
        .map_err(|_error| (error_code::INVALID_REQUEST, DETAIL_MALFORMED_JSON))?;
    if request
        .name
        .as_ref()
        .is_some_and(|name| name.is_empty() || name.len() > 200)
    {
        return Err((error_code::INVALID_REQUEST, DETAIL_NAME_INVALID));
    }
    if request
        .source_repo
        .as_deref()
        .is_some_and(|repo| Repository::parse(repo).is_err())
    {
        return Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_REPO_INVALID));
    }
    if request
        .source_ref
        .as_deref()
        .is_some_and(|revision| !valid_revision(revision))
    {
        return Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_REF_INVALID));
    }
    validate_reasons(request.required_credentials_reasons.as_ref())?;
    Ok(LibraryPatch::new(
        request.name.map(Cow::into_owned),
        request.description.map(Cow::into_owned),
        request.source_repo.map(Cow::into_owned),
        request.source_ref.map(Cow::into_owned),
        request.required_credentials_reasons,
        request.published,
    ))
}

fn validate_reasons(
    reasons: Option<&serde_json::Value>,
) -> Result<(), (error_code::ErrorCode, &'static str)> {
    if let Some(reasons) = reasons {
        let Some(reasons) = reasons.as_object() else {
            return Err((error_code::INVALID_REQUEST, DETAIL_REASONS_INVALID));
        };
        if reasons.len() > 32 {
            return Err((error_code::INVALID_REQUEST, DETAIL_REASONS_TOO_MANY));
        }
        for (credential, reason) in reasons {
            let Some(reason) = reason.as_str() else {
                return Err((error_code::INVALID_REQUEST, DETAIL_REASONS_INVALID));
            };
            if credential.len() > 200 || reason.len() > 500 {
                return Err((error_code::INVALID_REQUEST, DETAIL_REASON_TOO_LONG));
            }
        }
    }
    Ok(())
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
    use super::*;

    #[test]
    fn patch_validation_covers_identity_and_reason_bounds() {
        assert_eq!(
            patch_request(br#"{"description":"new"}"#).map(|_patch| ()),
            Ok(())
        );
        assert_eq!(
            patch_request(b""),
            Err((error_code::INVALID_REQUEST, DETAIL_BODY_REQUIRED))
        );
        assert_eq!(
            patch_request(br#"{"source_repo":"owner/repo/extra"}"#),
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_REPO_INVALID))
        );
        assert_eq!(
            patch_request(br#"{"source_ref":"../main"}"#),
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_REF_INVALID))
        );
        assert_eq!(
            patch_request(br#"{"required_credentials_reasons":[]}"#),
            Err((error_code::INVALID_REQUEST, DETAIL_REASONS_INVALID))
        );
        assert_eq!(
            patch_request(br#"{"required_credentials_reasons":{"github":42}}"#),
            Err((error_code::INVALID_REQUEST, DETAIL_REASONS_INVALID))
        );
    }
}
