//! Workspace draft HTTP boundaries; identity comes from the authenticated person.

pub(crate) mod input;
pub(crate) mod message;
pub(crate) mod models;
pub(crate) mod patch;
pub(crate) mod read;
pub(crate) mod render;
pub(crate) mod stream;
pub(crate) mod write;

use crate::handler::Refusal;
use crate::{
    auth::{Owned, PersonIdentity},
    services::Services,
};
use afd_authoring::Access;
use afd_core::id::Uuid7;
use axum::{response::Response, routing::MethodRouter};
use http::{HeaderValue, header};
use serde::Deserialize;
use std::sync::Arc;

/// The 404 every draft-member route answers: a missing, foreign or
/// creator-private draft is one answer, so none of them is disclosed.
#[cfg(feature = "openapi")]
const DRAFT_UNAVAILABLE: &str = "Private draft unavailable";

#[derive(Deserialize)]
pub(crate) struct DraftPath {
    draft_id: String,
}

fn id(path: &DraftPath) -> Result<Uuid7, Refusal> {
    Uuid7::parse(&path.draft_id).map_err(|_invalid| Refusal::malformed("draft_id must be a UUIDv7"))
}

fn access<'a>(owned: &'a Owned, person: &'a PersonIdentity) -> Access<'a> {
    Access {
        workspace: &owned.workspace,
        subject: person.subject(),
    }
}

/// Keeps private text and resource versions out of shared response caches.
pub(crate) fn private<D: Services>(router: MethodRouter<Arc<D>>) -> MethodRouter<Arc<D>> {
    router.layer(axum::middleware::map_response(mark_private))
}

async fn mark_private(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

fn attach(response: &mut Response, tag: &str) {
    if let Ok(tag) = HeaderValue::from_str(tag) {
        response.headers_mut().insert(header::ETAG, tag);
    }
}

fn location(response: &mut Response, workspace: &Uuid7, draft: &Uuid7, suffix: &str) {
    let path = format!("/v1/workspaces/{workspace}/drafts/{draft}{suffix}");
    if let Ok(path) = HeaderValue::from_str(&path) {
        response.headers_mut().insert(header::LOCATION, path);
    }
}
