//! Confirmed work is read independently of model-generated conversation text.

use std::{borrow::Cow, sync::Arc};

use af_core::paging::Cursor;
use afd_api_wire::fleet::{DurableTaskResponse, DurableTaskSummary, DurableTasksResponse};
use afd_fleet::durable_task::{DurableTaskPage, DurableTaskScope};
use afd_http::handler::Refusal;
use axum::{
    Json,
    extract::{Path, RawQuery, State},
    response::{IntoResponse as _, Response},
};
use headers::{ETag, HeaderMapExt as _, IfNoneMatch};
use http::{HeaderMap, StatusCode, header};

use super::input::{self, DurableTaskPath};
use crate::{
    auth::WorkspaceContext,
    handler::fleet::detail::{FleetPath, parse_fleet_id},
    services::TenantPlane,
};

const EVENT_READ: &str = "fleet_task_read_failed";

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/durable_tasks",
    tag = afd_http::openapi::tag::FLEETS, operation_id = "list_fleet_durable_tasks",
    summary = "List unfinished fleet work",
    description = "Returns unfinished durable tasks newest first. Durable task details remain readable after completion. An empty page does not imply that a conversation request has completed.",
    params(afd_http::openapi::path::Fleet,
        ("starting_after" = Option<String>, Query, description = "Opaque next_cursor from the previous page."),
        ("limit" = Option<u32>, Query, minimum = 1, maximum = 100, description = "Page size; defaults to 20.")),
    responses((status = 200, body = DurableTasksResponse<'_>, description = "Current work"),
        (status = 400, description = "Invalid page"), (status = 404, description = "Workspace or fleet unavailable"),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn list<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let scope = DurableTaskScope {
        workspace_id: owned.workspace,
        fleet_id: parse_fleet_id(&fleet_id)?,
    };
    let (after, limit) = input::page(query.as_deref().unwrap_or_default(), services.pages().list)?;
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
    get, path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/durable_tasks/{durable_task_id}",
    tag = afd_http::openapi::tag::FLEETS, operation_id = "get_fleet_durable_task",
    summary = "Read confirmed durable task progress and success criteria",
    description = "Returns the accepted objective, immutable success criteria, generation and cumulative limits. The strong ETag identifies the current native resource version.",
    params(afd_http::openapi::path::Fleet,
        ("durable_task_id" = String, Path, description = "Durable task UUIDv7."),
        ("If-None-Match" = Option<String>, Header, description = "Previously returned resource tag.")),
    responses((status = 200, body = DurableTaskResponse<'_>, description = "Durable task detail", headers(("ETag" = String))),
        (status = 304, description = "Resource unchanged", headers(("ETag" = String))),
        (status = 400, description = "Invalid durable task identifier"), (status = 404, description = "Durable task unavailable"),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn one<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(DurableTaskPath {
        fleet_id,
        durable_task_id,
    }): Path<DurableTaskPath>,
    headers: HeaderMap,
) -> Result<Response, Refusal> {
    let scope = DurableTaskScope {
        workspace_id: owned.workspace,
        fleet_id: parse_fleet_id(&fleet_id)?,
    };
    let task = input::task(&durable_task_id)?;
    let saved = services
        .tasks()
        .read(&scope, &task)
        .await
        .map_err(Refusal::at(EVENT_READ))?;
    let tag = saved.etag();
    let unchanged = headers
        .typed_get::<IfNoneMatch>()
        .zip(tag.parse::<ETag>().ok())
        .is_some_and(|(condition, current)| !condition.precondition_passes(&current));
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

fn render(page: DurableTaskPage) -> DurableTasksResponse<'static> {
    let next_cursor = page.more.then(|| page.items.last()).flatten().map(|last| {
        Cow::Owned(
            Cursor::Timestamp {
                at: last.created_at,
                id: last.id.clone(),
            }
            .to_string(),
        )
    });
    DurableTasksResponse {
        items: page
            .items
            .into_iter()
            .map(|item| DurableTaskSummary {
                id: Cow::Owned(item.id),
                title: Cow::Owned(item.title),
                status: item.state,
                resource_version: item.resource_version,
                generation: item.generation,
                created_at: item.created_at,
                expires_at: item.expires_at,
            })
            .collect(),
        next_cursor,
    }
}

pub(super) fn render_one(saved: afd_fleet::durable_task::DurableTaskSnapshot) -> Response {
    let tag = saved.etag();
    (
        [
            (header::ETAG, tag.clone()),
            (header::CACHE_CONTROL, "private, no-cache".to_owned()),
        ],
        Json(DurableTaskResponse {
            dependency: saved.dependency,
            publication: saved.publication,
            etag: Cow::Owned(tag),
            title: Cow::Owned(saved.title),
            resource_version: saved.resource_version,
            context: saved.context,
        }),
    )
        .into_response()
}
