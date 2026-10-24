//! Runner history HTTP adapter.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use afd_api_wire::admin::RunnerEventsResponse;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse as _, Response};

use super::query;
use crate::handler::{malformed, parse_id, refuse};
use crate::services::Services;

const EVENT: &str = "runner_events_failed";

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/fleets/runners/{runner_id}/events",
    tag = afd_http::openapi::tag::FLEET,
    operation_id = "list_fleet_runner_events",
    summary = "List runner activity",
    description = concat!(
        "Lists a runner's recorded activity newest first. Requires platform administrator ",
        "access. Filter by event type or an occurrence-time range in milliseconds.",
    ),
    params(
        afd_http::openapi::path::Runner,
        afd_http::openapi::query::OperatorPage,
        ("event_type" = Option<String>, Query, description = "Filter to one runner event type, or a comma-separated set returning the union (multi-value equality). Allowed tags: runner_registered, runner_online, runner_offline, lease_acquired, lease_released, runner_cordoned, runner_draining, runner_drained, runner_revoked. An unrecognised tag or an empty value refuses the whole request."),
        ("since" = Option<String>, Query, description = "Lower bound on occurred_at, an RFC 3339 instant (inclusive)."),
        ("until" = Option<String>, Query, description = "Upper bound on occurred_at, an RFC 3339 instant (inclusive). Must be >= since."),
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = RunnerEventsResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    Path(raw): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let runner = match parse_id(&raw, query::DETAIL_BAD_RUNNER_ID) {
        Ok(runner) => runner,
        Err(refused) => return refused.into_response(),
    };
    let query = match query::events(&params) {
        Ok(query) => query,
        Err(detail) => return malformed(detail),
    };
    match services
        .runners()
        .runner_events(&runner, &query.filter, query.cursor.as_ref(), query.limit)
        .await
    {
        Ok(rows) => {
            let total = rows.total();
            let next_cursor = rows
                .next_cursor()
                .map(|cursor| Cow::Owned(query::format(cursor)));
            Json(RunnerEventsResponse {
                items: rows.into_items(),
                total,
                next_cursor,
            })
            .into_response()
        }
        Err(error) => refuse(&error, EVENT),
    }
}
