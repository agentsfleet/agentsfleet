//! Newest-first runner lease history for operators.

use std::borrow::Cow;

use afd_api_wire::operator::{LeaseKind, LeaseOutcome, RunnerLeaseItem, RunnerLeasesResponse};
use afd_core::event::status::{FLEET_ERROR, PROCESSED};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_state::sql::{LEASE_STATUS_ACTIVE, LEASE_STATUS_EXPIRED, LEASE_STATUS_REPORTED};
use jiff::Timestamp;

use crate::error::{Result, cursor_invalid, query, runner_not_found};
use crate::sql;

const CONTEXT_TOTAL: &str = "runner lease total";
const CONTEXT_CURSOR: &str = "runner lease cursor";
const CONTEXT_PAGE: &str = "runner lease page";

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
        let total: i64 = sqlx::query_scalar(sql::SELECT_RUNNER_LEASE_TOTAL)
            .bind(runner.as_str())
            .bind(workspace)
            .bind(fleet)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_TOTAL))?
            .ok_or_else(runner_not_found)?;

        let boundary = match starting_after {
            Some(cursor) => Some((
                sqlx::query_scalar::<_, jiff_sqlx::Timestamp>(sql::SELECT_RUNNER_LEASE_CURSOR)
                    .bind(cursor.as_str())
                    .bind(runner.as_str())
                    .bind(workspace)
                    .bind(fleet)
                    .fetch_optional(&mut *connection)
                    .await
                    .map_err(query(CONTEXT_CURSOR))?
                    .ok_or_else(cursor_invalid)?,
                cursor.as_str(),
            )),
            None => None,
        };

        let items = sqlx::query_as::<_, LeaseRow>(sql::SELECT_RUNNER_LEASE_PAGE)
            .bind(runner.as_str())
            .bind(workspace)
            .bind(fleet)
            .bind(i64::from(limit))
            .bind(boundary.map(|(created_at, _)| created_at))
            .bind(boundary.map(|(_, id)| id))
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_PAGE))?
            .into_iter()
            .map(LeaseRow::into_item)
            .collect::<Vec<_>>();
        let next_cursor = (items.len() == limit as usize)
            .then(|| items.last().map(|item| item.id.clone()))
            .flatten();
        Ok(RunnerLeasesResponse {
            items,
            total,
            next_cursor,
        })
    }
}

/// One page row, by the names [`sql::SELECT_RUNNER_LEASE_PAGE`] gives its
/// columns.
#[derive(Debug, sqlx::FromRow)]
struct LeaseRow {
    id: String,
    fleet_id: String,
    fleet_name: String,
    workspace_id: String,
    event_id: String,
    event_type: String,
    actor: String,
    lease_status: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    lease_expires_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    fencing_token: i64,
    provider: String,
    model: String,
    posture: String,
    metered_input_token_count: i64,
    metered_cached_token_count: i64,
    metered_output_token_count: i64,
    event_status: String,
    failure_reason: Option<String>,
    failure_detail: Option<String>,
    wall_ms: Option<i64>,
    is_reclaim: bool,
}

impl LeaseRow {
    /// The row as the operator API reads it.
    fn into_item(self) -> RunnerLeaseItem<'static> {
        RunnerLeaseItem {
            outcome: outcome(&self.lease_status, Some(self.event_status.as_str())),
            id: Cow::Owned(self.id),
            fleet_id: Cow::Owned(self.fleet_id),
            fleet_name: Some(Cow::Owned(self.fleet_name)),
            workspace_id: Cow::Owned(self.workspace_id),
            event_id: Cow::Owned(self.event_id),
            event_type: Cow::Owned(self.event_type),
            actor: Cow::Owned(self.actor),
            failure_label: self.failure_reason.map(Cow::Owned),
            failure_detail: self.failure_detail.map(Cow::Owned),
            kind: if self.is_reclaim {
                LeaseKind::Reclaim
            } else {
                LeaseKind::Fresh
            },
            fencing_token: self.fencing_token,
            provider: Cow::Owned(self.provider),
            model: Cow::Owned(self.model),
            posture: Cow::Owned(self.posture),
            metered_input_tokens: self.metered_input_token_count,
            metered_cached_tokens: self.metered_cached_token_count,
            metered_output_tokens: self.metered_output_token_count,
            wall_ms: self.wall_ms,
            lease_expires_at: self.lease_expires_at,
            created_at: self.created_at,
        }
    }
}

fn outcome(lease_status: &str, event_status: Option<&str>) -> LeaseOutcome {
    match lease_status {
        LEASE_STATUS_EXPIRED => LeaseOutcome::Expired,
        LEASE_STATUS_ACTIVE => LeaseOutcome::Running,
        LEASE_STATUS_REPORTED => match event_status {
            Some(PROCESSED) => LeaseOutcome::Succeeded,
            Some(FLEET_ERROR) => LeaseOutcome::Failed,
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
        assert_eq!(outcome(LEASE_STATUS_ACTIVE, None), LeaseOutcome::Running);
        assert_eq!(
            outcome(LEASE_STATUS_ACTIVE, Some(PROCESSED)),
            LeaseOutcome::Running
        );
        assert_eq!(
            outcome(LEASE_STATUS_EXPIRED, Some(PROCESSED)),
            LeaseOutcome::Expired
        );
        assert_eq!(
            outcome(LEASE_STATUS_REPORTED, Some(PROCESSED)),
            LeaseOutcome::Succeeded
        );
        assert_eq!(
            outcome(LEASE_STATUS_REPORTED, Some(FLEET_ERROR)),
            LeaseOutcome::Failed
        );
        assert_eq!(outcome(LEASE_STATUS_REPORTED, None), LeaseOutcome::Unknown);
        assert_eq!(outcome("mystery", None), LeaseOutcome::Unknown);
    }

    #[test]
    fn the_stored_spellings_map_as_the_rows_hold_them() {
        // pin test: literal is the contract — the column bytes the lease and
        // event writers store.
        assert_eq!(
            outcome("reported", Some("processed")),
            LeaseOutcome::Succeeded
        );
        // pin test: literal is the contract
        assert_eq!(
            outcome("reported", Some("fleet_error")),
            LeaseOutcome::Failed
        );
        // pin test: literal is the contract
        assert_eq!(outcome("active", None), LeaseOutcome::Running);
        // pin test: literal is the contract
        assert_eq!(outcome("expired", None), LeaseOutcome::Expired);
    }
}
