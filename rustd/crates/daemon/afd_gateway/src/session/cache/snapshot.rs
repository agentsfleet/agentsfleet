//! The database row stays here; cache callers receive a validated, all-or-nothing snapshot.

use crate::{Result, error::ErrorKind};
use af_core::id::Uuid7;
use af_wire::protocol::LEASES_MAX;
use afd_db::Db;
use afd_fleet::lease::{Offered, Reported, Target};
use jiff::Timestamp;
use std::collections::HashMap;

/// A target exists only for fleet work; authoring uses its separate private progress service.
#[derive(Debug, Clone)]
pub(super) struct LeaseRecord {
    pub(super) id: Uuid7,
    pub(super) expires_at: Timestamp,
    pub(super) target: Option<Target>,
    pub(super) cancelled: bool,
    /// What the lease's report reads, kept from the offer (M245 §9).
    pub(super) reported: Option<Reported>,
}

impl From<Offered> for LeaseRecord {
    fn from(offered: Offered) -> Self {
        Self {
            id: offered.lease_id,
            expires_at: offered.expires_at,
            target: offered.target,
            cancelled: false,
            reported: offered.reported,
        }
    }
}

#[derive(sqlx::FromRow)]
struct LiveLeaseRow {
    lease_id: Uuid7,
    expires_at: jiff_sqlx::Timestamp,
    fleet_id: Option<Uuid7>,
    event_id: Option<Uuid7>,
    created_at: jiff_sqlx::Timestamp,
    event_created_at: Option<jiff_sqlx::Timestamp>,
    fencing_token: i64,
    cancelled: bool,
}

impl TryFrom<&LiveLeaseRow> for LeaseRecord {
    type Error = crate::Error;

    fn try_from(row: &LiveLeaseRow) -> Result<Self> {
        let target = match (&row.fleet_id, &row.event_id, &row.event_created_at) {
            (Some(fleet), Some(event), Some(event_created_at)) => Some(Target {
                fleet_id: fleet.clone(),
                event_id: event.clone(),
                lease_created_at: row.created_at.to_jiff(),
                event_created_at: event_created_at.to_jiff(),
                timing_eligible: !row.cancelled,
                fence: row.fencing_token,
            }),
            (None, None, None) => None,
            _ => return Err(ErrorKind::Protocol.into()),
        };
        Ok(Self {
            id: row.lease_id.clone(),
            expires_at: row.expires_at.to_jiff(),
            target,
            cancelled: row.cancelled,
            reported: None,
        })
    }
}

#[derive(garde::Validate)]
struct BoundedRows {
    #[garde(length(max = LEASES_MAX))]
    rows: Vec<LiveLeaseRow>,
}

/// Constructed only after every row fits the protocol's host bound and typed target shape.
pub(super) struct LeaseSnapshot(HashMap<Uuid7, LeaseRecord>);

impl TryFrom<Vec<LiveLeaseRow>> for LeaseSnapshot {
    type Error = crate::Error;

    fn try_from(rows: Vec<LiveLeaseRow>) -> Result<Self> {
        let rows = garde::Unvalidated::new(BoundedRows { rows }).validate()?;
        let records = rows
            .rows
            .iter()
            .map(|row| {
                let record = LeaseRecord::try_from(row)?;
                Ok((record.id.clone(), record))
            })
            .collect::<Result<_>>()?;
        Ok(Self(records))
    }
}

impl LeaseSnapshot {
    /// Limit plus one detects inconsistent excess rows without truncating valid ownership silently.
    pub(super) async fn load(database: &Db, runner: &Uuid7) -> Result<Self> {
        let rows =
            sqlx::query_as::<_, LiveLeaseRow>(include_str!("../sql/select_runner_live_leases.sql"))
                .bind(runner)
                .bind(afd_fleet::lease::sql::LEASE_STATUS_ACTIVE)
                .bind(i64::try_from(LEASES_MAX + 1)?)
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

    pub(super) fn into_records(self) -> HashMap<Uuid7, LeaseRecord> {
        self.0
    }
}

#[cfg(test)]
impl LeaseSnapshot {
    pub(super) fn from_records(records: impl IntoIterator<Item = LeaseRecord>) -> Self {
        Self(
            records
                .into_iter()
                .map(|record| (record.id.clone(), record))
                .collect(),
        )
    }
}
