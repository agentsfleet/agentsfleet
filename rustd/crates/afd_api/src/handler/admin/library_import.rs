//! Platform Fleet-library source onboarding HTTP adapter.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::error_code;
use afd_library::{ImportBody, PreparedBundle, SourceKind, valid_revision};
use afd_wire::admin::{AdminLibraryCreated, AdminLibraryImport, AdminLibraryRequirements};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use crate::auth::PersonIdentity;
use crate::envelope::ProblemResponse;
use crate::handler::{refuse, reject};
use crate::request_id::RequestId;
use crate::services::Services;

const SOURCE_UPLOAD: &str = "upload";
const SOURCE_GITHUB: &str = "github";
const SOURCE_TEMPLATE: &str = "template";
const VISIBILITY_PLATFORM: &str = "platform";
const DETAIL_BODY_REQUIRED: &str = "A request body is required";
const DETAIL_MALFORMED_JSON: &str = "The request body is not valid JSON";
const DETAIL_SOURCE_KIND: &str = "source_kind must be template, upload, or github";
const DETAIL_MISSING_SKILL: &str = "missing_skill";
const DETAIL_UPLOAD_ATTACHMENTS: &str =
    "upload sources cannot carry support files; use a github or template source";
const DETAIL_UPLOAD_REVISION: &str = "upload sources cannot carry a repository ref";
const DETAIL_TEMPLATE_REVISION: &str = "template sources cannot carry a repository ref";
const DETAIL_SOURCE_REF: &str = "source_ref must be 'owner/repo' for a github source";
const DETAIL_COLLISION: &str = "That bundle's name is already taken by a different repository. Rename the bundle, or retry with replace to overwrite it.";

/// Fetches or accepts one bundle, validates it, and stages its row as draft.
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    body: Bytes,
) -> Response {
    let request = match request(&body) {
        Ok(request) => request,
        Err((code, detail)) => return reject(code, detail),
    };
    let now = services.now();
    let result = match request.source_kind.as_ref() {
        SOURCE_UPLOAD => {
            let Some(skill) = request.skill_markdown else {
                return reject(error_code::FLEET_BUNDLE_INVALID, DETAIL_MISSING_SKILL);
            };
            let input = ImportBody {
                source_kind: SourceKind::Upload,
                source_ref: request.source_ref.into_owned(),
                source_revision: None,
                skill_markdown: skill.into_owned().into_bytes(),
                trigger_markdown: request
                    .trigger_markdown
                    .map(|markdown| markdown.into_owned().into_bytes()),
                support_files: Vec::new(),
            };
            services
                .library_imports()
                .upload(&input, request.replace, now)
                .await
        }
        SOURCE_GITHUB => {
            services
                .library_imports()
                .github(
                    request.source_ref.as_ref(),
                    request.revision.as_deref(),
                    request.replace,
                    now,
                )
                .await
        }
        SOURCE_TEMPLATE => {
            services
                .library_imports()
                .template(request.source_ref.as_ref(), request.replace, now)
                .await
        }
        _ => return reject(error_code::FLEET_BUNDLE_INVALID, DETAIL_SOURCE_KIND),
    };
    match result {
        Ok(bundle) => {
            tracing::info!(
                actor_id = identity.subject(),
                library_id = bundle.name,
                event = "admin_library_imported",
            );
            (StatusCode::CREATED, Json(created(bundle))).into_response()
        }
        Err(error) => match error.collision_incumbent() {
            Some(incumbent) => ProblemResponse::conflict(
                error_code::CATALOG_ID_COLLISION,
                DETAIL_COLLISION,
                RequestId::mint(),
                incumbent,
            )
            .into_response(),
            None => refuse(&error, "admin_library_import_failed"),
        },
    }
}

fn request(body: &[u8]) -> Result<AdminLibraryImport<'_>, (error_code::ErrorCode, &'static str)> {
    if body.is_empty() {
        return Err((error_code::INVALID_REQUEST, DETAIL_BODY_REQUIRED));
    }
    let request = afd_core::json::object_from_slice::<AdminLibraryImport<'_>>(body)
        .map_err(|_error| (error_code::INVALID_REQUEST, DETAIL_MALFORMED_JSON))?;
    match request.source_kind.as_ref() {
        SOURCE_UPLOAD if !request.support_files.is_empty() => {
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_UPLOAD_ATTACHMENTS))
        }
        SOURCE_UPLOAD if request.revision.is_some() => {
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_UPLOAD_REVISION))
        }
        SOURCE_TEMPLATE if request.revision.is_some() => {
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_TEMPLATE_REVISION))
        }
        SOURCE_TEMPLATE if !valid_revision(request.source_ref.as_ref()) => {
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_SOURCE_REF))
        }
        SOURCE_GITHUB
            if afd_library::Repository::parse(request.source_ref.as_ref()).is_err()
                || request
                    .revision
                    .as_deref()
                    .is_some_and(|revision| !valid_revision(revision)) =>
        {
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_SOURCE_REF))
        }
        SOURCE_UPLOAD | SOURCE_GITHUB | SOURCE_TEMPLATE => Ok(request),
        _ => Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_SOURCE_KIND)),
    }
}

fn created(bundle: PreparedBundle) -> AdminLibraryCreated<'static> {
    let requirements = bundle.requirements;
    AdminLibraryCreated {
        id: Cow::Owned(bundle.name.clone()),
        name: Cow::Owned(bundle.name),
        visibility: Cow::Borrowed(VISIBILITY_PLATFORM),
        content_hash: Cow::Owned(bundle.content_hash),
        requirements: AdminLibraryRequirements {
            credentials: requirements
                .credentials
                .into_iter()
                .map(Cow::Owned)
                .collect(),
            tools: requirements.tools.into_iter().map(Cow::Owned).collect(),
            network_hosts: requirements
                .network_hosts
                .into_iter()
                .map(Cow::Owned)
                .collect(),
            trigger_present: requirements.trigger_present,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_request_rejects_source_mismatches_before_io() {
        assert_eq!(
            request(br#"{"source_kind":"upload","skill_markdown":"---"}"#)
                .map(|request| request.source_kind.into_owned()),
            Ok(SOURCE_UPLOAD.to_owned())
        );
        assert_eq!(
            request(br#"{"source_kind":"upload","skill_markdown":"---","ref":"main"}"#)
                .map(|_request| ()),
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_UPLOAD_REVISION))
        );
        assert_eq!(
            request(br#"{"source_kind":"github","source_ref":"owner/repo/extra"}"#)
                .map(|_request| ()),
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_SOURCE_REF))
        );
    }
}
