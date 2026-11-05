//! Confirmed work is read independently of model-generated conversation text.

use std::{borrow::Cow, sync::Arc};

use afd_api_wire::fleet::{TaskResponse, TaskSummary, TasksResponse};
use afd_core::paging::Cursor;
use afd_fleet::task::{TaskPage, TaskScope};
use afd_http::handler::Refusal;
use axum::{
    Json,
    extract::{Path, RawQuery, State},
    response::{IntoResponse as _, Response},
};
use http::{HeaderMap, StatusCode, header};

use super::input::{self, TaskPath};
use crate::{
    auth::WorkspaceContext,
    handler::fleet::detail::{FleetPath, parse_fleet_id},
    services::Services,
};

const EVENT_READ: &str = "fleet_task_read_failed";

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/tasks",
    tag = afd_http::openapi::tag::FLEETS, operation_id = "list_fleet_tasks",
    summary = "List unfinished fleet work",
    description = "Returns unfinished tasks newest first. Task details remain readable after completion. An empty page does not imply that a conversation request has completed.",
    params(afd_http::openapi::path::Fleet,
        ("starting_after" = Option<String>, Query, description = "Opaque next_cursor from the previous page."),
        ("limit" = Option<u32>, Query, minimum = 1, maximum = 100, description = "Page size; defaults to 20.")),
    responses((status = 200, body = TasksResponse, description = "Current work"),
        (status = 400, description = "Invalid page"), (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"), (status = 404, description = "Workspace or fleet unavailable"),
        (status = 429, description = "Too many requests"), (status = 500, description = "Internal failure"),
        (status = 503, description = "Dependency unavailable"))
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let scope = TaskScope {
        workspace_id: owned.workspace,
        fleet_id: parse_fleet_id(&fleet_id)?,
    };
    let (after, limit) = input::page(query.as_deref().unwrap_or_default())?;
    let page = services
        .tasks()
        .list(&scope, after.as_ref(), limit)
        .await
        .map_err(Refusal::at(EVENT_READ))?;
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(render(page)),
    )
        .into_response())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/tasks/{task_id}",
    tag = afd_http::openapi::tag::FLEETS, operation_id = "get_fleet_task",
    summary = "Read confirmed task progress and success criteria",
    description = "Returns the accepted objective, immutable success criteria, generation and cumulative limits. The strong ETag identifies the current native resource version.",
    params(afd_http::openapi::path::Fleet,
        ("task_id" = String, Path, description = "Task UUIDv7."),
        ("If-None-Match" = Option<String>, Header, description = "Previously returned resource tag.")),
    responses((status = 200, body = TaskResponse, description = "Task detail", headers(("ETag" = String))),
        (status = 304, description = "Resource unchanged", headers(("ETag" = String))),
        (status = 400, description = "Invalid task identifier"), (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"), (status = 404, description = "Task unavailable"),
        (status = 429, description = "Too many requests"), (status = 500, description = "Internal failure"),
        (status = 503, description = "Dependency unavailable"))
))]
pub(crate) async fn one<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(TaskPath { fleet_id, task_id }): Path<TaskPath>,
    headers: HeaderMap,
) -> Result<Response, Refusal> {
    let scope = TaskScope {
        workspace_id: owned.workspace,
        fleet_id: parse_fleet_id(&fleet_id)?,
    };
    let task = input::task(&task_id)?;
    let saved = services
        .tasks()
        .read(&scope, &task)
        .await
        .map_err(Refusal::at(EVENT_READ))?;
    let tag = saved.etag();
    let unchanged = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| afd_http::etag::matches_if_none_match(value, &tag));
    let response = if unchanged {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        render_one(saved)
    };
    Ok((
        [
            (header::ETAG, tag),
            (header::CACHE_CONTROL, "private, no-cache".to_owned()),
        ],
        response,
    )
        .into_response())
}

fn render(page: TaskPage) -> TasksResponse<'static> {
    let next_cursor = page.more.then(|| page.items.last()).flatten().map(|last| {
        Cow::Owned(
            Cursor::Timestamp {
                at: last.created_at,
                id: last.id.clone(),
            }
            .to_string(),
        )
    });
    TasksResponse {
        items: page
            .items
            .into_iter()
            .map(|item| TaskSummary {
                id: Cow::Owned(item.id),
                title: Cow::Owned(item.title),
                status: item.state,
                resource_version: item.resource_version,
                generation: item.generation,
                accepted_revision_id: Cow::Owned(item.accepted_revision_id),
                created_at: item.created_at,
                expires_at: item.expires_at,
            })
            .collect(),
        next_cursor,
    }
}

pub(super) fn render_one(saved: afd_fleet::task::TaskSnapshot) -> Response {
    let tag = saved.etag();
    (
        [
            (header::ETAG, tag.clone()),
            (header::CACHE_CONTROL, "private, no-cache".to_owned()),
        ],
        Json(TaskResponse {
            dependency: saved.dependency,
            publication: saved.publication,
            etag: Cow::Owned(tag),
            title: Cow::Owned(saved.title),
            resource_version: saved.resource_version,
            setup_revision: saved.accepted_revision_number,
            context: saved.context,
        }),
    )
        .into_response()
}
