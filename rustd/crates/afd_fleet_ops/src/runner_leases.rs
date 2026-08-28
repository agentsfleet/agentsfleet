//! Newest-first runner lease history for operators.

use std::borrow::Cow;

use afd_core::id::Uuid7;
use afd_db::Db;
use afd_wire::operator::{LeaseKind, LeaseOutcome, RunnerLeaseItem, RunnerLeasesResponse};
use sqlx::Row as _;

use crate::error::{Result, cursor_invalid, query, row, runner_not_found};
use crate::sql;

const CONTEXT_TOTAL: &str = "runner lease total";
const CONTEXT_CURSOR: &str = "runner lease cursor";
const CONTEXT_PAGE: &str = "runner lease page";
const COLUMN_CREATED_AT: &str = "created_at";
const LEASE_ACTIVE: &str = "active";
const LEASE_REPORTED: &str = "reported";
const LEASE_EXPIRED: &str = "expired";
const EVENT_PROCESSED: &str = "processed";
const EVENT_FLEET_ERROR: &str = "fleet_error";

pub(crate) const DETAIL_BAD_CURSOR: &str = "starting_after must be a lease id held by this runner, and must match workspace_id and fleet when those filters are set";

/// Read-only runner lease history over an API-role pool.
#[derive(Debug, Clone)]
pub struct RunnerLeaseHistory {
    database: Db,
}

impl RunnerLeaseHistory {
    /// Builds the projection over an already-connected pool.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// Lists a filtered page and its filtered total.
    ///
    /// # Errors
    /// Reports a missing runner, a cursor outside this filtered stream, a row
    /// this projection cannot decode, or an unavailable datastore.
    pub async fn list(
        &self,
        runner: &Uuid7,
        workspace: Option<&Uuid7>,
        fleet: Option<&str>,
        starting_after: Option<&Uuid7>,
        limit: u32,
    ) -> Result<RunnerLeasesResponse<'static>> {
        let mut connection = self.database.acquire().await?;
        let workspace = workspace.map(Uuid7::as_str);
        let total: i64 = sqlx::query(sql::SELECT_RUNNER_LEASE_TOTAL)
            .bind(runner.as_str())
            .bind(workspace)
            .bind(fleet)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_TOTAL))?
            .ok_or_else(runner_not_found)?
            .try_get(0)
            .map_err(row("total"))?;

        let boundary = match starting_after {
            Some(cursor) => Some(
                sqlx::query(sql::SELECT_RUNNER_LEASE_CURSOR)
                    .bind(cursor.as_str())
                    .bind(runner.as_str())
                    .bind(workspace)
                    .bind(fleet)
                    .fetch_optional(&mut *connection)
                    .await
                    .map_err(query(CONTEXT_CURSOR))?
                    .ok_or_else(cursor_invalid)?
                    .try_get::<i64, _>(0)
                    .map_err(row(COLUMN_CREATED_AT))?,
            ),
            None => None,
        };

        let rows = match (boundary, starting_after) {
            (Some(created_at), Some(cursor)) => {
                sqlx::query(sql::SELECT_RUNNER_LEASE_PAGE_AFTER)
                    .bind(runner.as_str())
                    .bind(workspace)
                    .bind(created_at)
                    .bind(cursor.as_str())
                    .bind(i64::from(limit))
                    .bind(fleet)
                    .fetch_all(&mut *connection)
                    .await
            }
            (None, None) => {
                sqlx::query(sql::SELECT_RUNNER_LEASE_PAGE_FIRST)
                    .bind(runner.as_str())
                    .bind(workspace)
                    .bind(i64::from(limit))
                    .bind(fleet)
                    .fetch_all(&mut *connection)
                    .await
            }
            _ => return Err(cursor_invalid()),
        }
        .map_err(query(CONTEXT_PAGE))?;
        let items = rows.iter().map(decode).collect::<Result<Vec<_>>>()?;
        let next_cursor = (items.len() == limit as usize)
            .then(|| items.last().map(|item| Cow::Owned(item.id.to_string())))
            .flatten();
        Ok(RunnerLeasesResponse {
            items,
            total,
            next_cursor,
        })
    }
}

fn decode(row_data: &sqlx::postgres::PgRow) -> Result<RunnerLeaseItem<'static>> {
    let lease_status: String = get(row_data, "lease_status")?;
    let event_status: Option<String> = get(row_data, "event_status")?;
    Ok(RunnerLeaseItem {
        id: owned(get(row_data, "id")?),
        fleet_id: owned(get(row_data, "fleet_id")?),
        fleet_name: optional_owned(get(row_data, "fleet_name")?),
        workspace_id: owned(get(row_data, "workspace_id")?),
        event_id: owned(get(row_data, "event_id")?),
        event_type: owned(get(row_data, "event_type")?),
        actor: owned(get(row_data, "actor")?),
        outcome: outcome(&lease_status, event_status.as_deref()),
        failure_label: optional_owned(get(row_data, "failure_label")?),
        failure_detail: optional_owned(get(row_data, "failure_detail")?),
        kind: if get(row_data, "is_reclaim")? {
            LeaseKind::Reclaim
        } else {
            LeaseKind::Fresh
        },
        fencing_token: get(row_data, "fencing_token")?,
        provider: owned(get(row_data, "provider")?),
        model: owned(get(row_data, "model")?),
        posture: owned(get(row_data, "posture")?),
        metered_input_tokens: get(row_data, "metered_input_tokens")?,
        metered_cached_tokens: get(row_data, "metered_cached_tokens")?,
        metered_output_tokens: get(row_data, "metered_output_tokens")?,
        wall_ms: get(row_data, "wall_ms")?,
        lease_expires_at: get(row_data, "lease_expires_at")?,
        created_at: get(row_data, COLUMN_CREATED_AT)?,
    })
}

fn get<'r, T>(row_data: &'r sqlx::postgres::PgRow, column: &'static str) -> Result<T>
where
    T: sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres>,
{
    row_data.try_get(column).map_err(row(column))
}

fn owned(value: String) -> Cow<'static, str> {
    Cow::Owned(value)
}

fn optional_owned(value: Option<String>) -> Option<Cow<'static, str>> {
    value.map(Cow::Owned)
}

fn outcome(lease_status: &str, event_status: Option<&str>) -> LeaseOutcome {
    match lease_status {
        LEASE_EXPIRED => LeaseOutcome::Expired,
        LEASE_ACTIVE => LeaseOutcome::Running,
        LEASE_REPORTED => match event_status {
            Some(EVENT_PROCESSED) => LeaseOutcome::Succeeded,
            Some(EVENT_FLEET_ERROR) => LeaseOutcome::Failed,
            Some(_) | None => LeaseOutcome::Unknown,
        },
        _ => LeaseOutcome::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stored_status_pair_has_one_closed_outcome() {
        assert_eq!(outcome(LEASE_ACTIVE, None), LeaseOutcome::Running);
        assert_eq!(
            outcome(LEASE_ACTIVE, Some(EVENT_PROCESSED)),
            LeaseOutcome::Running
        );
        assert_eq!(
            outcome(LEASE_EXPIRED, Some(EVENT_PROCESSED)),
            LeaseOutcome::Expired
        );
        assert_eq!(
            outcome(LEASE_REPORTED, Some(EVENT_PROCESSED)),
            LeaseOutcome::Succeeded
        );
        assert_eq!(
            outcome(LEASE_REPORTED, Some(EVENT_FLEET_ERROR)),
            LeaseOutcome::Failed
        );
        assert_eq!(outcome(LEASE_REPORTED, None), LeaseOutcome::Unknown);
        assert_eq!(outcome("mystery", None), LeaseOutcome::Unknown);
    }
}
