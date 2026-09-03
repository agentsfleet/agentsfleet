//! Runner list and detail HTTP adapters.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use afd_runner::{RunnerDetail as StoredDetail, RunnerItem as StoredItem};
use afd_wire::operator::{RunnerDetail, RunnerItem, RunnersResponse};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse as _, Response};

use super::query;
use crate::handler::{malformed, refuse};
use crate::services::Services;

const EVENT_LIST: &str = "runner_list_failed";
const EVENT_DETAIL: &str = "runner_detail_failed";

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/fleets/runners",
    tag = afd_http::openapi::tag::FLEET,
    operation_id = "list_fleet_runners",
    summary = "List fleet runners",
    description = concat!(
        "Platform-admin operator-plane read of the whole fleet, newest first ",
        "over the composite (created_at, id) key. Under keyset pagination a ",
        "runner enrolled mid-traversal never repeats or hides a row. The ",
        "retired page, page_size and sort parameters are refused. Each row ",
        "carries a derived `liveness` — never the stored auth state, never ",
        "the token hash. ",
    ),
    params(
        afd_http::openapi::query::OperatorPage,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = RunnersResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let page = match query::page(&params) {
        Ok(page) => page,
        Err(detail) => return malformed(detail),
    };
    match services
        .runners()
        .list_runners(page.cursor.as_ref(), page.limit, services.now())
        .await
    {
        Ok(rows) => Json(RunnersResponse {
            items: rows.items().iter().map(item).collect(),
            total: rows.total(),
            next_cursor: rows
                .next_cursor()
                .map(|cursor| Cow::Owned(query::format(cursor))),
        })
        .into_response(),
        Err(error) => refuse(&error, EVENT_LIST),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/fleets/runners/{runner_id}",
    tag = afd_http::openapi::tag::FLEET,
    operation_id = "get_fleet_runner",
    summary = "Get a fleet runner",
    description = concat!(
        "Platform-admin read of a single runner. Carries the summary fields, ",
        "a live-work snapshot, and lifetime counters from durable lease and ",
        "event rows — never from in-memory metrics. The runner detail page ",
        "loads from this read. ",
    ),
    params(
        afd_http::openapi::path::Runner,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = RunnerDetail),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn detail<D: Services>(
    State(services): State<Arc<D>>,
    Path(raw): Path<String>,
) -> Response {
    let runner = match query::runner_id(&raw) {
        Ok(runner) => runner,
        Err(detail) => return malformed(detail),
    };
    match services
        .runners()
        .runner_detail(&runner, services.now())
        .await
    {
        Ok(row) => Json(detail_payload(&row)).into_response(),
        Err(error) => refuse(&error, EVENT_DETAIL),
    }
}

fn item(row: &StoredItem) -> RunnerItem<'static> {
    RunnerItem {
        id: Cow::Owned(row.id().to_string()),
        host_id: Cow::Owned(row.host_id().to_owned()),
        sandbox_tier: Cow::Owned(row.sandbox_tier().to_owned()),
        admin_state: row.admin_state(),
        liveness: row.liveness(),
        labels: row.labels().iter().cloned().map(Cow::Owned).collect(),
        last_seen_at: row.last_seen_at(),
        created_at: row.created_at(),
        assigned_policy: row.assigned_policy().cloned(),
        achievable: row.achievable().cloned(),
        degraded: row.is_degraded(),
        degraded_reason: row
            .degraded_reason()
            .map(|reason| Cow::Owned(reason.to_owned())),
    }
}

fn detail_payload(row: &StoredDetail) -> RunnerDetail<'static> {
    RunnerDetail {
        item: item(row.item()),
        active_lease_count: row.active_lease_count(),
        active_fleet_count: row.active_fleet_count(),
        leases_acquired: row.leases_acquired(),
        leases_succeeded: row.leases_succeeded(),
        leases_failed: row.leases_failed(),
        leases_expired: row.leases_expired(),
        selftest_requested_at: row.selftest_requested_at(),
        selftest_completed_at: row.selftest_completed_at(),
        selftest: row.selftest().cloned(),
    }
}
