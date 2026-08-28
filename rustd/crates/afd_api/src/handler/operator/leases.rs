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
