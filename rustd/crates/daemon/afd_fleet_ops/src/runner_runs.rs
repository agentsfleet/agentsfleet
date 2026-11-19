//! Newest-first runner run history for operators.

use std::borrow::Cow;

use af_core::event::status::{FLEET_ERROR, PROCESSED};
use af_core::id::Uuid7;
use afd_api_wire::operator::{
    RunDestination, RunKind, RunOutcome, RunnerRunItem, RunnerRunsResponse,
};
use afd_db::Db;
use afd_state::sql::{RUN_STATUS_ACTIVE, RUN_STATUS_EXPIRED, RUN_STATUS_REPORTED};
use jiff::Timestamp;

use crate::error::{Result, cursor_invalid, query, runner_not_found};
use crate::sql;

const CONTEXT_TOTAL: &str = "runner run total";
const CONTEXT_CURSOR: &str = "runner run cursor";
const CONTEXT_PAGE: &str = "runner run page";

pub(crate) const DETAIL_BAD_CURSOR: &str = "starting_after must be a run id held by this runner, and must match workspace_id and fleet when those filters are set";

/// Read-only runner run history over an API-role pool.
#[derive(Debug, Clone)]
pub struct RunnerRunHistory {
    database: Db,
}

impl RunnerRunHistory {
    /// Builds the projection over an already-connected pool.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// Lists a filtered page and its filtered total, with each run's
    /// destination as `viewer` (the tenant the caller acts in) may open it.
    ///
    /// # Errors
    /// Reports a missing runner, a cursor outside this filtered stream, a row
    /// this projection cannot decode, or an unavailable datastore.
    pub async fn list(
        &self,
        viewer: &Uuid7,
        runner: &Uuid7,
        workspace: Option<&Uuid7>,
        fleet: Option<&str>,
        starting_after: Option<&Uuid7>,
        limit: u32,
    ) -> Result<RunnerRunsResponse<'static>> {
        let mut connection = self.database.acquire().await?;
        // The fleet filter is a fleet's name, or its id when it reads as one.
        let fleet_id = fleet.and_then(|text| Uuid7::parse(text).ok());
        let fleet_id = fleet_id.as_ref();
        let total: i64 = sqlx::query_scalar(sql::SELECT_RUNNER_RUN_TOTAL)
            .bind(runner)
            .bind(workspace)
            .bind(fleet)
            .bind(fleet_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_TOTAL))?
            .ok_or_else(runner_not_found)?;

        let boundary = match starting_after {
            Some(cursor) => Some((
                sqlx::query_scalar::<_, jiff_sqlx::Timestamp>(sql::SELECT_RUNNER_RUN_CURSOR)
                    .bind(cursor)
                    .bind(runner)
                    .bind(workspace)
                    .bind(fleet)
                    .bind(fleet_id)
                    .fetch_optional(&mut *connection)
                    .await
                    .map_err(query(CONTEXT_CURSOR))?
                    .ok_or_else(cursor_invalid)?,
                cursor,
            )),
            None => None,
        };

        // One row past the page proves another page follows; a full page alone
        // does not, since the stream may end exactly on it.
        let mut rows = sqlx::query_as::<_, RunRow>(sql::SELECT_RUNNER_RUN_PAGE)
            .bind(runner)
            .bind(workspace)
            .bind(fleet)
            .bind(i64::from(limit) + 1)
            .bind(boundary.map(|(created_at, _)| created_at))
            .bind(boundary.map(|(_, id)| id))
            .bind(afd_billing::sql::work::FLEET_RUN)
            .bind(fleet_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_PAGE))?;
        let page = usize::try_from(limit).unwrap_or(usize::MAX);
        let more = rows.len() > page;
        rows.truncate(page);
        let items = rows
            .into_iter()
            .map(|row| row.into_item(viewer))
            .collect::<Vec<_>>();
        let next_cursor = more
            .then(|| items.last().map(|item| item.id.clone()))
            .flatten();
        Ok(RunnerRunsResponse {
            items,
            total,
            next_cursor,
        })
    }
}

/// One page row, by the names [`sql::SELECT_RUNNER_RUN_PAGE`] gives its
/// columns.
#[derive(Debug, sqlx::FromRow)]
struct RunRow {
    id: Uuid7,
    fleet_id: Uuid7,
    fleet_name: Option<String>,
    fleet_exists: bool,
    workspace_id: Uuid7,
    tenant_id: Uuid7,
    event_id: Uuid7,
    event_type: Option<String>,
    actor: Option<String>,
    run_status: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    run_expires_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    fencing_token: i64,
    provider: String,
    model: String,
    posture: String,
    metered_input_token_count: i64,
    metered_cached_token_count: i64,
    metered_output_token_count: i64,
    event_status: Option<String>,
    failure_reason: Option<String>,
    failure_detail: Option<String>,
    wall_ms: Option<i64>,
    is_reclaim: bool,
}

impl RunRow {
    /// The row as the operator API reads it, for `viewer`.
    fn into_item(self, viewer: &Uuid7) -> RunnerRunItem<'static> {
        RunnerRunItem {
            outcome: outcome(&self.run_status, self.event_status.as_deref()),
            destination: destination(self.fleet_exists, self.tenant_id.as_str(), viewer),
            id: Cow::Owned(self.id.into()),
            fleet_id: Cow::Owned(self.fleet_id.into()),
            fleet_name: self.fleet_name.map(Cow::Owned),
            workspace_id: Cow::Owned(self.workspace_id.into()),
            event_id: Cow::Owned(self.event_id.into()),
            event_type: self.event_type.map(Cow::Owned),
            actor: self.actor.map(Cow::Owned),
            failure_label: self.failure_reason.map(Cow::Owned),
            failure_detail: self.failure_detail.map(Cow::Owned),
            kind: if self.is_reclaim {
                RunKind::Reclaim
            } else {
                RunKind::Fresh
            },
            fencing_token: self.fencing_token,
            provider: Cow::Owned(self.provider),
            model: Cow::Owned(self.model),
            posture: Cow::Owned(self.posture),
            metered_input_tokens: self.metered_input_token_count,
            metered_cached_tokens: self.metered_cached_token_count,
            metered_output_tokens: self.metered_output_token_count,
            wall_ms: self.wall_ms,
            run_expires_at: self.run_expires_at,
            created_at: self.created_at,
        }
    }
}

/// Where an operator can go from a run: its fleet, if it still exists and
/// sits in the account the operator is acting in. A name proves neither.
fn destination(fleet_exists: bool, owner: &str, viewer: &Uuid7) -> RunDestination {
    if !fleet_exists {
        RunDestination::FleetDeleted
    } else if owner == viewer.as_str() {
        RunDestination::Available
    } else {
        RunDestination::NotPermitted
    }
}

fn outcome(run_status: &str, event_status: Option<&str>) -> RunOutcome {
    match run_status {
        RUN_STATUS_EXPIRED => RunOutcome::Expired,
        RUN_STATUS_ACTIVE => RunOutcome::Running,
        RUN_STATUS_REPORTED => match event_status {
            Some(PROCESSED) => RunOutcome::Succeeded,
            Some(FLEET_ERROR) => RunOutcome::Failed,
            Some(_) | None => RunOutcome::Unknown,
        },
        _ => RunOutcome::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stored_status_pair_has_one_closed_outcome() {
        assert_eq!(outcome(RUN_STATUS_ACTIVE, None), RunOutcome::Running);
        assert_eq!(
            outcome(RUN_STATUS_ACTIVE, Some(PROCESSED)),
            RunOutcome::Running
        );
        assert_eq!(
            outcome(RUN_STATUS_EXPIRED, Some(PROCESSED)),
            RunOutcome::Expired
        );
        assert_eq!(
            outcome(RUN_STATUS_REPORTED, Some(PROCESSED)),
            RunOutcome::Succeeded
        );
        assert_eq!(
            outcome(RUN_STATUS_REPORTED, Some(FLEET_ERROR)),
            RunOutcome::Failed
        );
        assert_eq!(outcome(RUN_STATUS_REPORTED, None), RunOutcome::Unknown);
        assert_eq!(outcome("mystery", None), RunOutcome::Unknown);
    }

    #[test]
    #[expect(
        clippy::panic,
        reason = "a fixed fixture id fails loudly when it is not canonical"
    )]
    fn a_destination_needs_a_live_fleet_in_the_viewers_own_account() {
        let viewer = Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010")
            .unwrap_or_else(|error| panic!("fixture id is canonical: {error}"));
        let other = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb011";
        for (exists, owner, expected) in [
            (true, viewer.as_str(), RunDestination::Available),
            (true, other, RunDestination::NotPermitted),
            (false, viewer.as_str(), RunDestination::FleetDeleted),
            (false, other, RunDestination::FleetDeleted),
        ] {
            assert_eq!(
                destination(exists, owner, &viewer),
                expected,
                "fleet present: {exists}, owned by {owner}"
            );
        }
    }

    #[test]
    fn the_stored_spellings_map_as_the_rows_hold_them() {
        // pin test: literal is the contract — the column bytes the run and
        // event writers store.
        assert_eq!(
            outcome("reported", Some("processed")),
            RunOutcome::Succeeded
        );
        // pin test: literal is the contract
        assert_eq!(outcome("reported", Some("fleet_error")), RunOutcome::Failed);
        // pin test: literal is the contract
        assert_eq!(outcome("active", None), RunOutcome::Running);
        // pin test: literal is the contract
        assert_eq!(outcome("expired", None), RunOutcome::Expired);
    }
}
