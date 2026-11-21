//! The database row stays here; cache callers receive a validated, all-or-nothing snapshot.

use crate::{Result, error::ErrorKind};
use af_core::id::Uuid7;
use af_wire::protocol::RUNS_MAX;
use afd_db::Db;
use afd_fleet::run::{AssignedRun, Reported, Target};
use jiff::Timestamp;
use std::collections::HashMap;

/// A target exists only for fleet work; a draft's run names its draft instead.
#[derive(Debug, Clone)]
pub(super) struct RunRecord {
    pub(super) id: Uuid7,
    pub(super) expires_at: Timestamp,
    pub(super) target: Option<Target>,
    pub(super) cancelled: bool,
    /// What the run's report reads, kept from the assignment.
    pub(super) reported: Option<Reported>,
    /// The draft a draft-owned run runs a turn of: its renewals, report and
    /// live answer take the draft's path.
    pub(super) draft: Option<Uuid7>,
}

impl From<AssignedRun> for RunRecord {
    fn from(assigned: AssignedRun) -> Self {
        Self {
            id: assigned.run_id,
            expires_at: assigned.expires_at,
            target: assigned.target,
            cancelled: false,
            reported: assigned.reported,
            draft: assigned.draft.map(|target| target.draft_id),
        }
    }
}

#[derive(sqlx::FromRow)]
struct LiveRunRow {
    run_id: Uuid7,
    expires_at: jiff_sqlx::Timestamp,
    fleet_id: Option<Uuid7>,
    turn_id: Option<Uuid7>,
    created_at: jiff_sqlx::Timestamp,
    turn_created_at: Option<jiff_sqlx::Timestamp>,
    fencing_token: i64,
    cancelled: bool,
    draft_id: Option<Uuid7>,
}

impl TryFrom<&LiveRunRow> for RunRecord {
    type Error = crate::Error;

    fn try_from(row: &LiveRunRow) -> Result<Self> {
        let target = match (&row.fleet_id, &row.turn_id, &row.turn_created_at) {
            (Some(fleet), Some(event), Some(turn_created_at)) => Some(Target {
                fleet_id: fleet.clone(),
                turn_id: event.clone(),
                run_created_at: row.created_at.to_jiff(),
                turn_created_at: turn_created_at.to_jiff(),
                timing_eligible: !row.cancelled,
                fence: row.fencing_token,
            }),
            // A draft's run: it names its draft, never a fleet.
            (None, None, None) if row.draft_id.is_some() => None,
            _ => return Err(ErrorKind::Protocol.into()),
        };
        Ok(Self {
            id: row.run_id.clone(),
            expires_at: row.expires_at.to_jiff(),
            target,
            cancelled: row.cancelled,
            reported: None,
            draft: row.draft_id.clone(),
        })
    }
}

#[derive(garde::Validate)]
struct BoundedRows {
    #[garde(length(max = RUNS_MAX))]
    rows: Vec<LiveRunRow>,
}

/// Constructed only after every row fits the protocol's host bound and typed target shape.
pub(super) struct RunSnapshot(HashMap<Uuid7, RunRecord>);

impl TryFrom<Vec<LiveRunRow>> for RunSnapshot {
    type Error = crate::Error;

    fn try_from(rows: Vec<LiveRunRow>) -> Result<Self> {
        let rows = garde::Unvalidated::new(BoundedRows { rows }).validate()?;
        let records = rows
            .rows
            .iter()
            .map(|row| {
                let record = RunRecord::try_from(row)?;
                Ok((record.id.clone(), record))
            })
            .collect::<Result<_>>()?;
        Ok(Self(records))
    }
}

impl RunSnapshot {
    /// Limit plus one detects inconsistent excess rows without truncating valid ownership silently.
    pub(super) async fn load(database: &Db, runner: &Uuid7) -> Result<Self> {
        let rows =
            sqlx::query_as::<_, LiveRunRow>(include_str!("../sql/select_runner_live_runs.sql"))
                .bind(runner)
                .bind(afd_state::sql::RUN_STATUS_ACTIVE)
                .bind(i64::try_from(RUNS_MAX + 1)?)
                .fetch_all(&mut *database.acquire().await?)
                .await?;
        rows.try_into()
    }

    pub(super) fn cancellations(&self) -> Vec<Uuid7> {
        self.0
            .values()
            .filter(|record| record.cancelled)
            .map(|record| record.id.clone())
            .collect()
    }

    pub(super) fn into_records(self) -> HashMap<Uuid7, RunRecord> {
        self.0
    }
}

#[cfg(test)]
impl RunSnapshot {
    pub(super) fn from_records(records: impl IntoIterator<Item = RunRecord>) -> Self {
        Self(
            records
                .into_iter()
                .map(|record| (record.id.clone(), record))
                .collect(),
        )
    }
}
