//! One bounded editor read; each private conversation is checked against its creator.

use super::{access, render};
use crate::auth::{PersonIdentity, WorkspaceContext};
use crate::{
    handler::{
        draft,
        fleet::detail::{FleetPath, parse_fleet_id},
    },
    services::{FleetSchedules as _, Services, WorkspaceEvents as _},
};
use afd_api_wire::authoring::{AuthoringContext, Conversation, CursorPage, SetupActivity};
use afd_http::handler::{Refusal, authoring_refusal};
use axum::{
    Json,
    extract::{Path, Query, State, rejection::QueryRejection},
    response::{IntoResponse as _, Response},
};
use serde::Deserialize;
use std::sync::Arc;

const INVALID_QUERY: &str = "Invalid authoring context query";
const READ_FAILED: &str = "fleet_authoring_activity_read_failed";

#[derive(Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContextQuery {
    #[serde(default)]
    #[garde(inner(length(bytes, min = 1, max = 4096)))]
    starting_after: Option<String>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/authoring-context",
    operation_id = "get_fleet_authoring_context", tag = afd_http::openapi::tag::FLEETS,
    summary = "Read your fleet editor context",
    description = "Returns current fleet setup, at most three creator-private conversations with at most 25 recent role rows each, and 25 filtered activity receipts. The complete response is limited to four mebibytes. Older whole turns or conversations move behind their continuation when that limit is reached. The conversation cursor binds creator, workspace and fleet. Older messages remain available through each draft's messages resource. Private responses must not be cached.",
    params(afd_http::openapi::path::Fleet,
        ("starting_after" = Option<String>, Query, description = "Use next_cursor unchanged to read older conversations for this creator and fleet.")),
    responses((status = 200, body = AuthoringContext, description = "Bounded private editor context"),
        (status = 400, description = "Invalid query or cursor"), (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"), (status = 404, description = "Fleet or private draft unavailable"),
        (status = 429, description = "Too many requests"), (status = 500, description = "Internal failure"),
        (status = 503, description = "Dependency unavailable"))
))]
pub(crate) async fn read<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    asked: Result<Query<ContextQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let Query(asked) = asked.map_err(|_invalid| Refusal::malformed(INVALID_QUERY))?;
    let asked = garde::Unvalidated::new(asked)
        .validate()
        .map_err(|_invalid| Refusal::malformed(INVALID_QUERY))?;
    let identity = access(&owned, &person);
    let conversations = async {
        services
            .creations()
            .conversations(identity, &fleet, asked.starting_after.clone())
            .await
            .map_err(authoring_refusal)
    };
    let receipts = activity_history(services.as_ref(), &owned.workspace, &fleet);
    let ((saved, schedules), conversations, receipts) = tokio::try_join!(
        current(services.as_ref(), identity, &fleet),
        conversations,
        receipts
    )?;
    let boundaries = super::context_budget::capture(&conversations.items);
    let mut payload = AuthoringContext {
        fleet: render::payload(services.as_ref(), saved, &schedules),
        conversations: conversations.items.into_iter().map(conversation).collect(),
        activities: activity_page(receipts),
        next_cursor: conversations.next_cursor,
    };
    super::context_budget::fit(&mut payload, boundaries)?;
    Ok(Json(payload).into_response())
}

async fn activity_history<D: Services>(
    services: &D,
    workspace: &afd_core::id::Uuid7,
    fleet: &afd_core::id::Uuid7,
) -> Result<Vec<afd_events::EventDetailRow>, Refusal> {
    services
        .events()
        .thread_for_fleet(
            workspace,
            fleet,
            None,
            26,
            Some(afd_events::activity::EVENT_TYPE_ACTIVITY),
        )
        .await
        .map_err(Refusal::at(READ_FAILED))
}

async fn current<D: Services>(
    services: &D,
    identity: afd_authoring::Access<'_>,
    fleet: &afd_core::id::Uuid7,
) -> Result<(afd_authoring::SavedFleet, Vec<afd_cron::Schedule>), Refusal> {
    let saved = services
        .creations()
        .read(identity, fleet)
        .await
        .map_err(authoring_refusal)?;
    let schedules = services
        .schedules()
        .list(identity.workspace, fleet)
        .await
        .map_err(Refusal::at(READ_FAILED))?;
    Ok((saved, schedules))
}

fn activity_page(receipts: Vec<afd_events::EventDetailRow>) -> CursorPage<SetupActivity> {
    let more = receipts.len() > 25;
    let activities = receipts
        .into_iter()
        .take(25)
        .map(|event| SetupActivity {
            event_id: event.row.event_id,
            event_type: event.row.event_type,
            status: event.row.status,
            request_json: event.request_json,
            created_at: event.row.created_at,
        })
        .collect::<Vec<_>>();
    let next = more
        .then(|| activities.last())
        .flatten()
        .map(|last| afd_events::Cursor::after(last.created_at, &last.event_id).encode());
    CursorPage {
        items: activities,
        total: None,
        next_cursor: next,
    }
}

fn conversation(
    value: afd_authoring::Conversation,
) -> Conversation<afd_fleet_runtime::Definition, afd_authoring::types::Review> {
    Conversation {
        draft: draft::render::draft(value.draft),
        messages: value
            .messages
            .into_iter()
            .map(draft::render::message)
            .collect(),
        messages_starting_after: value.messages_starting_after,
        truncated: value.truncated,
    }
}
