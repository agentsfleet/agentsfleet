//! Runner lease-history HTTP adapter.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse as _, Response};

use super::query;
use crate::auth::PersonIdentity;
use crate::handler::{malformed, parse_id, refuse};
use crate::services::Services;

const EVENT: &str = "runner_leases_failed";

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/fleets/runners/{runner_id}/leases",
    tag = afd_http::openapi::tag::FLEET,
    operation_id = "list_fleet_runner_leases",
    summary = "List a runner's work assignments",
    description = concat!(
        "Lists a runner's work assignments newest first, with fleet outcomes and failure ",
        "reasons. Requires platform administrator access. Continue with `next_cursor` as ",
        "`starting_after`.\n\n",
        "`destination` is `available` for a fleet in your account, `fleet_deleted` after ",
        "deletion, or `not_permitted` in another account. Fleet and event fields become ",
        "null when their records are removed.\n\n",
        "Completed assignments remain for 30 days after settlement. Running or renewing ",
        "assignments are retained. Assignments abandoned by a dead runner expire after the ",
        "same window, then receive their own retention window.\n\n",
        "Acquisition and release activity remains for 30 days after each event; other ",
        "runner lifecycle activity is retained. Lifetime counters remain unchanged when ",
        "history is removed.",
    ),
    params(
        afd_http::openapi::path::Runner,
        ("starting_after" = Option<String>, Query, description = concat!(
            "Lease identifier from this runner's previous `next_cursor`. It must match the ",
            "current workspace and fleet filters. If retention removed it, restart from the ",
            "first page.",
        )),
        ("workspace_id" = Option<String>, Query, description = "Narrow the page and total to leases owned by one workspace. A malformed id is refused; an unknown one matches nothing."),
        ("fleet" = Option<String>, Query, description = concat!(
            "Filter by fleet identifier or exact name, ignoring case. Combines with ",
            "`workspace_id`. Empty or oversized values are refused; unknown fleets return no ",
            "results.",
        )),
        ("limit" = Option<String>, Query, description = "Rows per page (1-100)."),
    ),
    responses(
        (status = 200, description = "One page of the runner's leases, newest first", body = afd_api_wire::operator::RunnerLeasesResponse),
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
    identity: PersonIdentity,
    Path(raw): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let runner = match parse_id(&raw, query::DETAIL_BAD_RUNNER_ID) {
        Ok(runner) => runner,
        Err(refused) => return refused.into_response(),
    };
    let query = match query::leases(&params) {
        Ok(query) => query,
        Err(detail) => return malformed(detail),
    };
    match services
        .runner_lease_history()
        .list(
            identity.person().tenant(),
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
