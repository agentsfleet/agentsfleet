//! Runner run-history HTTP adapter.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse as _, Response};

use super::query;
use crate::OperatorPlane;
use crate::auth::PersonIdentity;
use crate::handler::{malformed, parse_id, refuse};

const EVENT: &str = "runner_runs_failed";

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/admin/runners/{runner_id}/runs",
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "list_platform_runner_runs",
    summary = "List a runner's work assignments",
    description = concat!(
        "Lists a runner's work assignments newest first, with fleet outcomes and failure ",
        "reasons. Requires platform administrator access. Continue with `next_cursor` as ",
        "`starting_after`.\n\n",
        "`destination` is `available` for a fleet in your account, `fleet_deleted` after ",
        "deletion, or `not_permitted` in another account. Fleet and event fields become ",
        "null when their records are removed.\n\n",
        "Assignments and runner activity are kept, whatever their state, until a retention ",
        "policy is chosen.",
    ),
    params(
        afd_http::openapi::path::Runner,
        ("starting_after" = Option<String>, Query, description = concat!(
            "Run identifier from this runner's previous `next_cursor`. It must match the ",
            "current workspace and fleet filters. If the run was removed, restart from the ",
            "first page.",
        )),
        ("workspace_id" = Option<String>, Query, description = "Narrow the page and total to runs owned by one workspace. A malformed id is refused; an unknown one matches nothing."),
        ("fleet" = Option<String>, Query, description = concat!(
            "Filter by fleet identifier or exact name, ignoring case. Combines with ",
            "`workspace_id`. Empty or oversized values are refused; unknown fleets return no ",
            "results.",
        )),
        ("limit" = Option<String>, Query, description = "Rows per page (1-100)."),
    ),
    responses(
        (status = 200, description = "One page of the runner's runs, newest first", body = afd_api_wire::operator::RunnerRunsResponse<'_>),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn list<D: OperatorPlane>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(raw): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let runner = match parse_id(&raw, query::DETAIL_BAD_RUNNER_ID) {
        Ok(runner) => runner,
        Err(refused) => return refused.into_response(),
    };
    let query = match query::runs(&params) {
        Ok(query) => query,
        Err(detail) => return malformed(detail),
    };
    match services
        .runner_run_history()
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
