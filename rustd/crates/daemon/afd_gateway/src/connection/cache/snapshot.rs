//! The database row stays here; cache callers receive a validated, all-or-nothing snapshot.

use crate::{Result, error::ErrorKind};
use af_core::id::Uuid7;
use af_wire::protocol::RUNS_MAX;
use afd_db::Db;
use afd_fleet::run::{AssignedRun, Reported, Target};
use jiff::Timestamp;
use std::collections::HashMap;

/// A live run and the fleet turn it serves.
#[derive(Debug, Clone)]
pub(super) struct RunRecord {
    pub(super) id: Uuid7,
    pub(super) expires_at: Timestamp,
    pub(super) target: Option<Target>,
    pub(super) cancelled: bool,
    /// What the run's report reads, kept from the assignment.
    pub(super) reported: Option<Reported>,
}

impl From<AssignedRun> for RunRecord {
    fn from(assigned: AssignedRun) -> Self {
        Self {
            id: assigned.run_id,
            expires_at: assigned.expires_at,
            target: assigned.target,
            cancelled: false,
            reported: assigned.reported,
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
            _ => return Err(ErrorKind::Protocol.into()),
        };
        Ok(Self {
            id: row.run_id.clone(),
            expires_at: row.expires_at.to_jiff(),
            target,
            cancelled: row.cancelled,
            reported: None,
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

#[cfg(test)]
mod tests {
    //! A rebuild's rows are fleet work, or refused whole.

    use super::{LiveRunRow, RunSnapshot};
    use crate::error::ErrorKind;
    use af_core::id::Uuid7;
    use jiff::Timestamp;

    type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

    fn id(index: u32) -> af_core::error::Result<Uuid7> {
        Uuid7::parse(&format!("019a3c00-0000-7000-8000-{index:012x}"))
    }

    /// A run row with no fleet and no turn: half of no shape.
    fn bare(run: Uuid7) -> LiveRunRow {
        LiveRunRow {
            run_id: run,
            expires_at: Timestamp::MAX.into(),
            fleet_id: None,
            turn_id: None,
            created_at: Timestamp::UNIX_EPOCH.into(),
            turn_created_at: None,
            fencing_token: 7,
            cancelled: false,
        }
    }

    #[test]
    fn a_fleet_run_carries_its_target_and_its_cancellation() -> TestResult {
        let run = id(1)?;
        let mut row = bare(run.clone());
        row.fleet_id = Some(id(2)?);
        row.turn_id = Some(id(3)?);
        row.turn_created_at = Some(Timestamp::UNIX_EPOCH.into());
        row.cancelled = true;
        let snapshot = RunSnapshot::try_from(vec![row])?;
        assert_eq!(snapshot.cancellations(), std::slice::from_ref(&run));
        let records = snapshot.into_records();
        let record = records.get(&run).ok_or("the fleet run was dropped")?;
        let target = record
            .target
            .as_ref()
            .ok_or("a fleet run carries its target")?;
        assert_eq!(target.fence, 7);
        assert!(!target.timing_eligible, "a cancelled run is not timed");
        assert_eq!(record.expires_at, Timestamp::MAX);
        Ok(())
    }

    #[test]
    fn a_row_with_part_of_a_fleet_target_refuses_the_whole_snapshot() -> TestResult {
        let mut half = bare(id(6)?);
        half.fleet_id = Some(id(3)?);
        let refused = RunSnapshot::try_from(vec![half]).err();
        assert!(
            refused
                .as_ref()
                .is_some_and(|error| matches!(error.kind(), ErrorKind::Protocol)),
            "{refused:?}"
        );
        let orphan = RunSnapshot::try_from(vec![bare(id(7)?)]).err();
        assert!(
            orphan
                .as_ref()
                .is_some_and(|error| matches!(error.kind(), ErrorKind::Protocol)),
            "{orphan:?}"
        );
        Ok(())
    }
}
