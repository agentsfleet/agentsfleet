//! Which fleets a workspace holds, for the viewers watching all of them.
//!
//! # Read on a notice, never on a timer
//!
//! A workspace stream reads the set when it opens and again when a notice says
//! the workspace gained or lost a fleet, or the account's members changed, or a
//! gap may have hidden one. Nothing re-reads it on a period, so nothing caches
//! it: a cached set would serve a fleet's own notice the set from before it.
//!
//! # What the set is NOT
//!
//! An authorization. The set says which fleets EXIST in a workspace; whether
//! this caller may see them is decided per request by the ownership layer,
//! against the caller's own credential.
//!
//! # The counters beside the set
//!
//! A `hello` names the set and where every fleet in it stands, so a subscriber
//! is right before its first frame arrives. [`Fleets::counters`] is asked only
//! when a `hello` is about to go out — on connect and on a change to the set —
//! never on a notice that finds the set steady.

use std::collections::{BTreeMap, BTreeSet};

use af_core::id::Uuid7;
use afd_api_wire::tail::FleetCounters;

use crate::error::{self, Result};
use crate::{Fleets, sql};

/// What a failure in this read is reported as having been doing.
const CONTEXT_LIVE_SET: &str = "enumerate a workspace's fleets";

/// What a failure in the counters read is reported as having been doing.
const CONTEXT_COUNTERS: &str = "read the counters of a workspace's fleets";

impl Fleets {
    /// Every fleet this workspace holds, by identifier.
    ///
    /// Sorted, because the set is announced to a client as a list and a set
    /// whose order changed between ticks would read as a set that changed.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A workspace holding no fleets
    /// is an empty set, not a failure — a wall with nothing on it is a state an
    /// operator reaches by deleting their last fleet.
    pub async fn live_set(&self, workspace: &Uuid7) -> Result<BTreeSet<String>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_scalar::<_, Uuid7>(sql::SELECT_FLEET_IDS)
            .bind(workspace)
            .fetch_all(connection.as_mut())
            .await
            .map(|ids| ids.into_iter().map(String::from).collect())
            .map_err(error::query(CONTEXT_LIVE_SET))
    }

    /// Where every fleet of `workspace` in `fleets` stands, by identifier.
    ///
    /// Every identifier the workspace holds answers — zeros for a fleet that
    /// has never run — so a client can assign each tile from the map without
    /// a fallback; one it does not hold is absent, never disclosed. Read fresh
    /// on every call; see the module note.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row this build cannot
    /// read.
    pub async fn counters(
        &self,
        workspace: &Uuid7,
        fleets: &[String],
    ) -> Result<BTreeMap<String, FleetCounters>> {
        // A requested identifier that is not a UUIDv7 names no fleet, so it is
        // absent like any other this workspace does not hold.
        let fleets: Vec<Uuid7> = fleets
            .iter()
            .filter_map(|fleet| Uuid7::parse(fleet).ok())
            .collect();
        let mut connection = self.database.acquire().await?;
        sqlx::query_as::<_, CounterRow>(sql::SELECT_FLEET_COUNTERS_FOR_SET)
            .bind(workspace)
            .bind(&fleets)
            .fetch_all(connection.as_mut())
            .await
            .map(|rows| rows.into_iter().map(CounterRow::into_entry).collect())
            .map_err(error::query(CONTEXT_COUNTERS))
    }
}

/// One row of [`sql::SELECT_FLEET_COUNTERS_FOR_SET`], named as the statement
/// names its columns.
#[derive(Debug, sqlx::FromRow)]
struct CounterRow {
    id: Uuid7,
    event_count: i64,
    budget_used_nanos: i64,
}

impl CounterRow {
    /// The map entry a wall assigns its tile from.
    fn into_entry(self) -> (String, FleetCounters) {
        let counters = FleetCounters {
            events_processed: self.event_count,
            budget_used_nanos: self.budget_used_nanos,
        };
        (self.id.into(), counters)
    }
}
