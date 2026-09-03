//! Platform Fleet-library source onboarding HTTP adapter.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_library::{Destination, Onboarded};
use afd_wire::admin::{AdminLibraryCreated, AdminLibraryRequirements};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use crate::auth::PersonIdentity;
use crate::envelope::ProblemResponse;
use crate::handler::{library_onboard, refuse, reject};
use crate::request_id::RequestId;
use crate::services::Services;

const VISIBILITY_PLATFORM: &str = "platform";
const DETAIL_COLLISION: &str = "That bundle's name is already taken by a different repository. Rename the bundle, or retry with replace to overwrite it.";

/// Fetches or accepts one bundle, validates it, and stages its row as draft.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/admin/fleet-libraries",
    tag = afd_http::openapi::tag::FLEET_LIBRARY,
    operation_id = "onboard_platform_fleet_library",
    summary = "Onboard a platform Fleet library entry",
    description = concat!(
        "Onboards a Fleet library entry into the global platform catalog from ",
        "a GitHub source reference. Requires the `platform-library:write` ",
        "scope. The canonical bundle is written to internal object storage ",
        "keyed by content hash; the response carries metadata only — never an ",
        "object-store key or support-file content. ",
    ),
    request_body = afd_wire::admin::AdminLibraryImport,
    responses(
        (status = 201, description = afd_http::openapi::CREATED, body = AdminLibraryCreated),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 502, description = afd_http::openapi::BAD_GATEWAY),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    body: Bytes,
) -> Response {
    let parsed = match library_onboard::parse(&body) {
        Ok(parsed) => parsed,
        Err((code, detail)) => return reject(code, detail),
    };
    let result = import(
        &*services,
        parsed.onboarding,
        services.now(),
        parsed.replace_requested,
    )
    .await;
    respond(result, &identity)
}

async fn import<D: Services>(
    services: &D,
    onboarding: library_onboard::Onboarding<'_>,
    now: UnixMillis,
    replace: bool,
) -> afd_library::Result<Onboarded> {
    // The operator-curated catalogue, and the only tier that takes a `replace`:
    // it is keyed by the bundle's own name, so a second source claiming an
    // existing one is a collision somebody may choose to force past. A
    // workspace's library is keyed by its content hash and has nothing to force.
    let into = Destination::Platform { replace };
    library_onboard::run(services.library_imports(), onboarding, into, now).await
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
    use afd_library::{ImportBody, SourceKind};

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
