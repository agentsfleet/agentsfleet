//! Domain projections move into wire types without copying instructions.

use af_fleet_runtime::Definition;
use afd_api_wire::plugin as wire;
use afd_authoring::types;
use axum::{
    Json,
    response::{IntoResponse as _, Response},
};
use http::{HeaderValue, StatusCode, header};

pub(super) fn card(value: types::PluginCard) -> wire::Card<types::Requirements> {
    wire::Card {
        id: value.id.to_string(),
        name: value.name,
        description: value.description,
        created_at: value.created_at,
        requirements: value.requirements,
        required_credentials_reasons: value.required_credentials_reasons,
        head_revision_id: value.head_revision_id.to_string(),
        head_revision: value.head_revision,
        etag: value.etag,
    }
}

pub(super) fn revision(
    value: types::PluginRevision,
) -> wire::Revision<Definition, types::Requirements> {
    wire::Revision {
        id: value.id.to_string(),
        plugin_id: value.plugin_id.to_string(),
        revision: value.revision,
        definition: value.definition,
        canonical_digest: value.canonical_digest,
        requirements: value.requirements,
        policy_semantics: value.policy_semantics,
        created_at: value.created_at,
    }
}

pub(super) fn plugin(value: types::Plugin) -> Response {
    let tag = value.etag.clone();
    let payload = wire::Plugin {
        card: card(value.card),
        head: revision(value.head),
        review: value.review,
        etag: value.etag,
    };
    let mut response = Json(payload).into_response();
    attach(&mut response, header::ETAG, &tag);
    response
}

pub(super) fn published(workspace: &af_core::id::Uuid7, value: types::Published) -> Response {
    let location = format!(
        "/v1/workspaces/{workspace}/plugins/{}/revisions/{}",
        value.plugin_id, value.revision_id
    );
    let tag = value.etag.clone();
    let payload = wire::Published {
        plugin_id: value.plugin_id.to_string(),
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
