//! Runner history HTTP adapter.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use afd_wire::admin::RunnerEventsResponse;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse as _, Response};

use super::query;
use crate::handler::{malformed, refuse};
use crate::services::Services;

const EVENT: &str = "runner_events_failed";

pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    Path(raw): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let runner = match query::runner_id(&raw) {
        Ok(runner) => runner,
        Err(detail) => return malformed(detail),
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
