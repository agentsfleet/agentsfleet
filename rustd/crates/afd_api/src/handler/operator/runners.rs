//! Runner list and detail HTTP adapters.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use afd_fleet::{RunnerDetail as StoredDetail, RunnerItem as StoredItem};
use afd_wire::operator::{RunnerDetail, RunnerItem, RunnersResponse};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse as _, Response};

use super::query;
use crate::handler::{malformed, refuse};
use crate::services::Services;

const EVENT_LIST: &str = "runner_list_failed";
const EVENT_DETAIL: &str = "runner_detail_failed";

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
            items: rows.items.iter().map(item).collect(),
            total: rows.total,
            next_cursor: rows
                .next_cursor
                .as_ref()
                .map(|cursor| Cow::Owned(query::format(cursor))),
        })
        .into_response(),
        Err(error) => refuse(&error, EVENT_LIST),
    }
}

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
        id: Cow::Owned(row.id.to_string()),
        host_id: Cow::Owned(row.host_id.clone()),
        sandbox_tier: Cow::Owned(row.sandbox_tier.clone()),
        admin_state: row.admin_state,
        liveness: row.liveness,
        labels: row.labels.iter().cloned().map(Cow::Owned).collect(),
        last_seen_at: row.last_seen_at,
        created_at: row.created_at,
        assigned_policy: row.assigned_policy.clone(),
        achievable: row.achievable.clone(),
        degraded: row.degraded,
        degraded_reason: row.degraded_reason.clone().map(Cow::Owned),
    }
}

fn detail_payload(row: &StoredDetail) -> RunnerDetail<'static> {
    RunnerDetail {
        item: item(&row.item),
        active_lease_count: row.active_lease_count,
        active_fleet_count: row.active_fleet_count,
        leases_acquired: row.leases_acquired,
        leases_succeeded: row.leases_succeeded,
        leases_failed: row.leases_failed,
        leases_expired: row.leases_expired,
        selftest_requested_at: row.selftest_requested_at,
        selftest_completed_at: row.selftest_completed_at,
        selftest: row.selftest.clone(),
    }
}
