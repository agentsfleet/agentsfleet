//! Runner lease-history HTTP adapter.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse as _, Response};

use super::query;
use crate::handler::{malformed, refuse};
use crate::services::Services;

const EVENT: &str = "runner_leases_failed";

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/fleets/runners/{runner_id}/leases",
    tag = afd_http::openapi::tag::FLEET,
    operation_id = "list_fleet_runner_leases",
    summary = "List a runner's leases",
    description = concat!(
        "Platform-admin read of a single runner's lease history, newest ",
        "first. Each lease is joined to its Fleet event, so outcome and ",
        "failure cause arrive in one round trip. Stripe-style keyset ",
        "pagination — `starting_after` is a lease id from a previous page's ",
        "`next_cursor`. Settled leases are retained for 30 days. The window ",
        "is measured from settlement, not from acquisition. A lease acquired ",
        "long ago and settled yesterday keeps its full window. A background ",
        "sweep deletes them past that window. A lease still running or ",
        "renewing is never swept. The per-lease activity records ",
        "(`lease_acquired`, `lease_released`) are swept on the same 30-day ",
        "window. Each record ages from when it occurred, not from its lease's ",
        "settlement. A long run's opening record can therefore be pruned ",
        "shortly before its lease row. The runner's lifecycle activity is ",
        "kept at any age, so a long-lived runner's feed never empties. A ",
        "lease whose runner died without reporting is marked expired once it ",
        "passes the same window. It then keeps its own window, like any ",
        "settled lease. The lifetime totals on `GET /v1/fleets/runners/{id}` ",
        "are unaffected by any of this. They count transitions as they ",
        "happen, not surviving rows. A long-lived runner's `leases_acquired` ",
        "will therefore exceed what this endpoint returns. ",
    ),
    params(
        afd_http::openapi::path::Runner,
        ("starting_after" = Option<String>, Query, description = "A lease id this runner holds; the page starts strictly after it. A lease id the runner does not hold is refused. So is one outside the `workspace_id` or `fleet` filters when those filters are set. The cursor names a position in the filtered stream, so it must belong to it. A cursor whose lease the retention sweep has since deleted is refused for the same reason — start again from the first page."),
        ("workspace_id" = Option<String>, Query, description = "Narrow the page and total to leases owned by one workspace. A malformed id is refused; an unknown one matches nothing."),
        ("fleet" = Option<String>, Query, description = "Narrow the page and total to leases run for one fleet, named by its id or its exact name (case-insensitive). Combines with `workspace_id`; the two filters intersect. An empty or over-long value is refused; a value no fleet matches returns nothing."),
        ("limit" = Option<String>, Query, description = "Rows per page (1-100)."),
    ),
    responses(
        (status = 200, description = "One page of the runner's leases, newest first", body = afd_wire::operator::RunnerLeasesResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    Path(raw): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let runner = match query::runner_id(&raw) {
        Ok(runner) => runner,
        Err(detail) => return malformed(detail),
    };
    let query = match query::leases(&params) {
        Ok(query) => query,
        Err(detail) => return malformed(detail),
    };
    match services
        .runner_lease_history()
        .list(
            &runner,
            query.workspace.as_ref(),
            query.fleet.as_deref(),
            query.starting_after.as_ref(),
            query.limit,
        )
        .await
    {
        Ok(page) => Json(page).into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}
