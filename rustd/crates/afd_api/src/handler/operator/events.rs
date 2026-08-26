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
    let page = match query::page(&params) {
        Ok(page) => page,
        Err(detail) => return malformed(detail),
    };
    match services
        .runners()
        .runner_events(&runner, page.cursor.as_ref(), page.limit)
        .await
    {
        Ok(rows) => Json(RunnerEventsResponse {
            items: rows.items,
            total: rows.total,
            next_cursor: rows
                .next_cursor
                .as_ref()
                .map(|cursor| Cow::Owned(query::format(cursor))),
        })
        .into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}
