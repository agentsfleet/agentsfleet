//! Counting the fleets, by status, for the gauge that reports them.
//!
//! # Why a sweeper and not a transition counter
//!
//! A gauge moved on every install, edit and purge would drift from the table
//! the first time a process died between the row write and the gauge update,
//! and nothing would ever reconcile it. A count sampled from the table that
//! owns the truth cannot drift: it is wrong for at most one interval, and the
//! next pass is right again. The cost is one grouped count every
//! [`FLEET_CENSUS_INTERVAL`], bounded by the status vocabulary rather than by
//! the table.
//!
//! # What a failed pass publishes
//!
//! Nothing. The cells are withdrawn, so the gauge shows a gap rather than a
//! zero — a zero would read as "no fleets", which nobody measured. A status
//! the closed set does not spell is reported and left out; guessing a member
//! for it would count a fleet under a state it is not in.

use std::time::Duration;

use afd_db::Db;
use afd_observability::metrics::label::fleet::FleetStatusLabel;
use afd_observability::producers::fleet::census;
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::sql;
use crate::sweep::{Sweep, Swept};

#[cfg(test)]
mod tests;

/// Statement name, for the context a query failure carries.
const CONTEXT_CENSUS: &str = "fleet census count";

/// The event a status this build does not model is reported under.
const EVENT_UNMODELLED_STATUS: &str = "fleet_status_unmodelled";

/// How long between census passes.
///
/// A fleet count moves on install and edit, both operator-paced; a tighter
/// cadence would spend a Postgres round trip on a number that did not change.
pub const FLEET_CENSUS_INTERVAL: Duration = Duration::from_secs(30);

/// One row of the grouped count: a stored spelling and how many rows carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counted {
    /// The `core.fleets.status` value, as stored.
    pub status: String,
    /// How many fleets carry it.
    pub fleets: u64,
}

/// What a pass made of the rows it counted.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// The counts this build can publish, one per modelled status returned.
    pub counts: Vec<(FleetStatusLabel, u64)>,
    /// The rows it could not: spellings the closed set does not carry.
    pub unmodelled: Vec<Counted>,
}

impl Tally {
    /// Every row the pass saw, modelled or not.
    #[must_use]
    pub fn scanned(&self) -> u64 {
        let modelled: u64 = self.counts.iter().map(|(_, fleets)| fleets).sum();
        let unmodelled: u64 = self.unmodelled.iter().map(|row| row.fleets).sum();
        modelled.saturating_add(unmodelled)
    }
}

/// Sorts the rows of a grouped count into what can be published and what
/// cannot.
///
/// Pure, so the one decision this sweeper makes — which spellings it will
/// count — is provable without a table.
#[must_use]
pub fn tally(rows: impl IntoIterator<Item = Counted>) -> Tally {
    rows.into_iter().fold(Tally::default(), |mut tally, row| {
        match FleetStatusLabel::from_spelling(&row.status) {
            Some(status) => tally.counts.push((status, row.fleets)),
            None => tally.unmodelled.push(row),
        }
        tally
    })
}

/// The census pass, over the api-role pool.
#[derive(Debug, Clone)]
pub struct Census {
    /// Where the rows are.
    database: Db,
}

impl Census {
    /// A sweeper counting through `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// The grouped count, one row per distinct status.
    async fn count(&self) -> Result<Vec<Counted>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::sweep::COUNT_FLEETS_BY_STATUS)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_CENSUS))?;
        rows.iter()
            .map(|row| {
                let status: String = row.try_get(0).map_err(query(CONTEXT_CENSUS))?;
                let fleets: i64 = row.try_get(1).map_err(query(CONTEXT_CENSUS))?;
                Ok(Counted {
                    status,
                    // `COUNT(*)` is never negative; the conversion exists for
                    // the type, not for a case.
                    fleets: u64::try_from(fleets).unwrap_or_default(),
                })
            })
            .collect()
    }
}

/// Publishes what a count answered, or withdraws the census when it did not.
///
/// The publish half of a pass, split from the count so a suite can drive it
/// with a failed count and prove the withdrawal without a table that fails.
fn settle(counted: Result<Vec<Counted>>) -> Result<Swept> {
    let rows = counted.inspect_err(|_failed| census::fleet_census_withdrawn())?;
    let tally = tally(rows);
    for row in &tally.unmodelled {
        let status = row.status.as_str();
        let fleets = row.fleets;
        tracing::warn!(
            status,
            fleets,
            event = EVENT_UNMODELLED_STATUS,
            "fleet rows hold a status this daemon does not model; they are left out of the census"
        );
    }
    census::fleet_census_observed(&tally.counts);
    Ok(Swept {
        scanned: tally.scanned(),
        changed: 0,
    })
}

impl Sweep for Census {
    fn name(&self) -> &'static str {
        "fleet-census"
    }

    fn interval(&self) -> Duration {
        FLEET_CENSUS_INTERVAL
    }

    async fn sweep(&self) -> Result<Swept> {
        settle(self.count().await)
    }
}
