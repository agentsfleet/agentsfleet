//! Platform Fleet-library source onboarding HTTP adapter.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_library::{Destination, ImportBody, InvalidBundle, Onboarded, SourceKind, valid_revision};
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
    let (kind, request) = match request(&body) {
        Ok(parsed) => parsed,
        Err((code, detail)) => return reject(code, detail),
    };
    let result = import(&*services, kind, request, services.now()).await;
    respond(result, &identity)
}

async fn import<D: Services>(
    services: &D,
    kind: SourceKind,
    request: AdminLibraryImport<'_>,
    now: UnixMillis,
) -> afd_library::Result<Onboarded> {
    // The operator-curated catalogue, and the only tier that takes a `replace`:
    // it is keyed by the bundle's own name, so a second source claiming an
    // existing one is a collision somebody may choose to force past. A
    // workspace's library is keyed by its content hash and has nothing to force.
    let into = Destination::Platform {
        replace: request.replace,
    };
    match kind {
        SourceKind::Upload => {
            let Some(skill) = request.skill_markdown else {
                return Err(afd_library::Error::Invalid(InvalidBundle::MissingSkill));
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
            services.library_imports().upload(&input, into, now).await
        }
        SourceKind::Github => {
            services
                .library_imports()
                .github(
                    request.source_ref.as_ref(),
                    request.revision.as_deref(),
                    into,
                    now,
                )
                .await
        }
        SourceKind::Template => {
            services
                .library_imports()
                .template(request.source_ref.as_ref(), into, now)
                .await
        }
    }
}

fn respond(result: afd_library::Result<Onboarded>, identity: &PersonIdentity) -> Response {
    match result {
        Ok(onboarded) => {
            let actor_id = identity.subject();
            // The id the CATALOGUE answered, not one re-derived from the
            // bundle: they agree on this tier and would not on the other.
            let library_id = onboarded.id.as_str();
            tracing::info!(actor_id, library_id, event = "admin_library_imported",);
            (StatusCode::CREATED, Json(created(onboarded))).into_response()
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

/// The request, with its source kind PARSED rather than checked.
///
/// The kind comes back as a [`SourceKind`] and not a string, so the dispatch
/// below is exhaustive over the three this daemon serves and carries no
/// unreachable arm — the spelling is `SourceKind`'s own, which is also what
/// stops this file and the store from drifting onto different ones (RULE UFS).
fn request(
    body: &[u8],
) -> Result<(SourceKind, AdminLibraryImport<'_>), (error_code::ErrorCode, &'static str)> {
    if body.is_empty() {
        return Err((error_code::INVALID_REQUEST, DETAIL_BODY_REQUIRED));
    }
    let request = afd_core::json::object_from_slice::<AdminLibraryImport<'_>>(body)
        .map_err(|_error| (error_code::INVALID_REQUEST, DETAIL_MALFORMED_JSON))?;
    let kind = SourceKind::parse(request.source_kind.as_ref())
        .ok_or((error_code::FLEET_BUNDLE_INVALID, DETAIL_SOURCE_KIND))?;

    let refusal = match kind {
        SourceKind::Upload if request.skill_markdown.is_none() => Some(DETAIL_MISSING_SKILL),
        SourceKind::Upload if !request.support_files.is_empty() => Some(DETAIL_UPLOAD_ATTACHMENTS),
        SourceKind::Upload if request.revision.is_some() => Some(DETAIL_UPLOAD_REVISION),
        SourceKind::Template if request.revision.is_some() => Some(DETAIL_TEMPLATE_REVISION),
        SourceKind::Template if !valid_revision(request.source_ref.as_ref()) => {
            Some(DETAIL_SOURCE_REF)
        }
        SourceKind::Github
            if afd_library::Repository::parse(request.source_ref.as_ref()).is_err()
                || request
                    .revision
                    .as_deref()
                    .is_some_and(|revision| !valid_revision(revision)) =>
        {
            Some(DETAIL_SOURCE_REF)
        }
        SourceKind::Upload | SourceKind::Github | SourceKind::Template => None,
    };
    match refusal {
        Some(detail) => Err((error_code::FLEET_BUNDLE_INVALID, detail)),
        None => Ok((kind, request)),
    }
}

fn created(onboarded: Onboarded) -> AdminLibraryCreated<'static> {
    let bundle = onboarded.bundle;
    let requirements = bundle.requirements;
    AdminLibraryCreated {
        id: Cow::Owned(onboarded.id),
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
    #![expect(
        clippy::expect_used,
        reason = "test fixture preparation should fail loudly"
    )]

    use super::*;

    #[test]
    fn import_request_rejects_source_mismatches_before_io() {
        // The kind comes back PARSED, so a caller downstream matches on a
        // closed set rather than on a string it has to re-check.
        assert_eq!(
            request(br#"{"source_kind":"upload","skill_markdown":"---"}"#).map(|(kind, _)| kind),
            Ok(SourceKind::Upload)
        );
        assert_eq!(
            request(br#"{"source_kind":"upload","skill_markdown":"---","ref":"main"}"#)
                .map(|_parsed| ()),
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_UPLOAD_REVISION))
        );
        assert_eq!(
            request(br#"{"source_kind":"github","source_ref":"owner/repo/extra"}"#)
                .map(|_parsed| ()),
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_SOURCE_REF))
        );

        for (body, expected) in [
            (b"".as_slice(), DETAIL_BODY_REQUIRED),
            (b"[]".as_slice(), DETAIL_MALFORMED_JSON),
            (br#"{"source_kind":"upload"}"#, DETAIL_MISSING_SKILL),
            // Carries the skill so the missing-skill guard above passes and the
            // attachment arm is the one that answers. Without it this case
            // silently graded the wrong refusal.
            (
                br#"{"source_kind":"upload","skill_markdown":"---","support_files":[{}]}"#,
                DETAIL_UPLOAD_ATTACHMENTS,
            ),
            (
                br#"{"source_kind":"template","source_ref":"reviewer","ref":"main"}"#,
                DETAIL_TEMPLATE_REVISION,
            ),
            (
                br#"{"source_kind":"template","source_ref":"bad/ref"}"#,
                DETAIL_SOURCE_REF,
            ),
            (
                br#"{"source_kind":"github","source_ref":"owner/repo","ref":"bad/ref"}"#,
                DETAIL_SOURCE_REF,
            ),
            (br#"{"source_kind":"unknown"}"#, DETAIL_SOURCE_KIND),
        ] {
            assert_eq!(
                request(body).map(|_parsed| ()),
                Err((
                    if expected == DETAIL_BODY_REQUIRED || expected == DETAIL_MALFORMED_JSON {
                        error_code::INVALID_REQUEST
                    } else {
                        error_code::FLEET_BUNDLE_INVALID
                    },
                    expected,
                ))
            );
        }
    }

    #[test]
    fn a_prepared_bundle_maps_every_requirement_to_the_created_wire_shape() {
        let input = ImportBody {
            source_kind: SourceKind::Upload,
            source_ref: "operator-upload".to_owned(),
            source_revision: None,
            skill_markdown: b"---\nname: reviewer\ndescription: Reviews changes\nversion: 1.0.0\n---\n"
                .to_vec(),
            trigger_markdown: Some(
                b"---\nname: reviewer\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: [bash]\n  credentials: [GITHUB_TOKEN]\n  network:\n    allow: [api.github.com]\n  budget:\n    daily_dollars: 1\n---\n"
                    .to_vec(),
            ),
            support_files: Vec::new(),
        };
        let bundle = afd_library::prepare(&input).expect("fixture bundle is valid");

        // The id the catalogue would have answered. On this tier it equals the
        // bundle's own name; the type is what stops that from being assumed.
        let response = created(Onboarded {
            id: bundle.name.clone(),
            bundle,
        });

        assert_eq!(response.id, "reviewer");
        assert_eq!(response.visibility, VISIBILITY_PLATFORM);
        assert_eq!(response.requirements.credentials, ["GITHUB_TOKEN"]);
        assert_eq!(response.requirements.tools, ["bash"]);
        assert_eq!(response.requirements.network_hosts, ["api.github.com"]);
        assert!(response.requirements.trigger_present);
    }
}
