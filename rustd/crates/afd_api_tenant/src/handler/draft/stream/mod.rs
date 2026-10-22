//! Private progress is reauthorized before every metadata frame, including reconnects.

mod body;

use super::{DraftPath, access, id};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    services::Services,
};
use afd_http::handler::{Refusal, authoring_refusal};
use axum::{
    extract::{Path, State},
    response::{IntoResponse as _, Response, Sse},
};
use std::sync::Arc;

/// Opens a bounded, creator-only hint stream; ordinary reads recover missed state.
#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/workspaces/{workspace_id}/drafts/{draft_id}/events/stream",
    operation_id = "stream_workspace_draft", tag = afd_http::openapi::tag::DRAFTS,
    summary = "Stream your private setup progress",
    description = "Private progress and provisional answer text. Reconcile the draft and messages on reconnect. Last-Event-ID is an ephemeral hint. Access is checked before every frame and at least every ten seconds while quiet.",
    params(afd_http::openapi::path::Draft),
    responses((status=200, body=String, content_type="text/event-stream", description="Private progress"),
        (status=400, description="Invalid draft identifier"), (status=401, description=afd_http::openapi::UNAUTHORIZED),
        (status=403, description=afd_http::openapi::FORBIDDEN), (status=404, description=super::DRAFT_UNAVAILABLE),
        (status=429, description=afd_http::openapi::TOO_MANY_REQUESTS), (status=500, description=afd_http::openapi::INTERNAL),
        (status=503, description="Stream capacity or dependency unavailable"))
))]
pub(crate) async fn open<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<DraftPath>,
) -> Result<Response, Refusal> {
    let draft = id(&path)?;
    let view = services
        .authoring()
        .initial_view(access(&owned, &person), &draft)
        .await
        .map_err(authoring_refusal)?;
    let live = services.live();
    let slot = live.admit().ok_or_else(|| {
        afd_observability::producers::http::stream_shed();
        Refusal::at_stream_ceiling(live.carrying(), live.capacity())
    })?;
    let subscription = live
        .hub()
        .map(|hub| hub.subscribe(&afd_authoring::draft_channel(&draft)));
    let stream = body::frames(
        services.authoring().clone(),
        owned.workspace,
        person.subject().to_owned(),
        draft,
        view.generation,
        slot,
        subscription,
    );
    Ok(Sse::new(stream).into_response())
}
