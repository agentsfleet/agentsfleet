//! Domain projections move into wire types without copying instructions.

use afd_api_wire::library as wire;
use afd_authoring::types;
use afd_fleet_runtime::Definition;
use axum::{
    Json,
    response::{IntoResponse as _, Response},
};
use http::{HeaderValue, StatusCode, header};

pub(super) fn card(value: types::LibraryCard) -> wire::Card<types::Requirements> {
    wire::Card {
        id: value.id,
        name: value.name,
        description: value.description,
        visibility: value.visibility,
        created_at: value.created_at,
        requirements: value.requirements,
        required_credentials_reasons: value.required_credentials_reasons,
        head_revision_id: value.head_revision_id.to_string(),
        etag: value.etag,
    }
}

pub(super) fn revision(
    value: types::LibraryRevision,
) -> wire::Revision<Definition, types::Requirements> {
    wire::Revision {
        id: value.id.to_string(),
        library_id: value.library_id,
        revision: value.revision,
        definition: value.definition,
        canonical_digest: value.canonical_digest,
        requirements: value.requirements,
        policy_semantics: value.policy_semantics,
        created_at: value.created_at,
    }
}

pub(super) fn library(value: types::Library) -> Response {
    let tag = value.etag.clone();
    let payload = wire::Library {
        card: card(value.card),
        head: revision(value.head),
        review: value.review,
        etag: value.etag,
    };
    let mut response = Json(payload).into_response();
    attach(&mut response, header::ETAG, &tag);
    response
}

pub(super) fn published(workspace: &afd_core::id::Uuid7, value: types::Published) -> Response {
    let location = format!(
        "/v1/workspaces/{workspace}/fleet-libraries/{}/revisions/{}",
        value.library_id, value.revision_id
    );
    let tag = value.etag.clone();
    let payload = wire::Published {
        library_id: value.library_id,
        revision_id: value.revision_id.to_string(),
        head_revision_id: value.head_revision_id.to_string(),
        etag: value.etag,
    };
    let mut response = (StatusCode::CREATED, Json(payload)).into_response();
    attach(&mut response, header::ETAG, &tag);
    attach(&mut response, header::LOCATION, &location);
    response
}

pub(super) fn attach(response: &mut Response, name: header::HeaderName, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        response.headers_mut().insert(name, value);
    }
}
