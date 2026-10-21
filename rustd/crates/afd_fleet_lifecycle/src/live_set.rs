//! Which fleets a workspace holds, for the viewers watching all of them.
//!
//! # Why this read is cached where the others are not
//!
//! Every other read here answers one request. This one answers a TICK: a live
//! workspace stream re-asks it every few seconds for as long as somebody has a
//! tab open, so V viewers of one workspace would otherwise run V copies of the
//! same query forever. The set is the same for all of them — it is a property
//! of the workspace, not of the caller — so one answer serves every viewer.
//!
//! # What the cache is NOT
//!
//! An authorization. The set says which fleets EXIST in a workspace; whether
//! this caller may see them is decided per request by the ownership layer,
//! every tick, against the caller's own credential. Caching the enumeration is
//! safe precisely because the permission is never cached with it.
//!
//! # Concurrent misses share one load
//!
//! When an entry expires under V viewers of one workspace, all V miss at
//! once. The read loads through `moka`'s `try_get_with`, which runs ONE
//! enumeration and hands every waiter its answer, so the expiry instant costs
//! one statement as the steady state does. A load that fails reaches every
//! waiter as the same failure behind an `Arc`, lifted into this crate's
//! `Shared` kind, which answers the code of the failure beneath it — an
//! outage stays an outage to each of them — and nothing is cached, so the
//! next read tries again.
//!
//! # Capacity counts fleets, not workspaces
//!
//! An entry weighs the fleets it holds, so the bound is on what is kept in
//! memory: one workspace with thousands of fleets evicts by its weight rather
//! than costing every small workspace its entry. A workspace with no fleets
//! still weighs one, so empty sets are bounded too. A set heavier than the
//! whole bound is never kept, so that workspace re-reads on every tick (its
//! concurrent readers still share one load). No single set is capped yet: the
//! product has no per-workspace fleet limit to cap it at.
//!
//! # The counters beside the set are NOT cached
//!
//! A `hello` names the set and where every fleet in it stands, so a subscriber
//! is right before its first frame arrives. The set may be ten seconds old and
//! nobody notices; a counter ten seconds old is the exact staleness the frame
//! exists to remove. So [`Fleets::counters`] runs its statement every time,
//! and it is asked only when a `hello` is about to go out — on connect and on
//! a change to the set — never on the tick that finds the set steady.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use afd_api_wire::tail::FleetCounters;
use afd_core::id::Uuid7;
use moka::future::Cache;

use crate::error::{self, Result};
use crate::{Fleets, sql};

/// What a failure in this read is reported as having been doing.
const CONTEXT_LIVE_SET: &str = "enumerate a workspace's fleets";

/// What a failure in the counters read is reported as having been doing.
const CONTEXT_COUNTERS: &str = "read the counters of a workspace's fleets";

/// How long an enumeration is served before it is read again.
///
/// The cadence a stream's refresh tick runs on, so a fleet installed while a
/// wall is open appears within one beat of it. Short enough that an operator
/// does not notice, long enough that a hundred tabs are one statement.
pub(crate) const LIVE_SET_TTL: Duration = Duration::from_secs(10);

/// How many fleet identifiers the cache holds across every workspace.
///
/// A bound on memory rather than a promise: an entry weighs its fleet count
/// (see [`weight`]), and evicting one costs the next tick a query. Sixteen
/// times the 4 096 workspaces the count-based bound held, at the handful of
/// fleets a workspace carries today.
pub(crate) const LIVE_SET_FLEETS: u64 = 65_536;

/// The cache the enumeration is served from.
pub(crate) type LiveSets = Cache<String, Arc<BTreeSet<String>>>;

/// A cache sized and aged for the stream refresh tick.
pub(crate) fn live_sets() -> LiveSets {
    Cache::builder()
        .max_capacity(LIVE_SET_FLEETS)
        .weigher(weight)
        .time_to_live(LIVE_SET_TTL)
        .build()
}

/// What one workspace's set weighs against [`LIVE_SET_FLEETS`]: its fleet
/// count, and never less than one.
fn weight(_workspace: &String, set: &Arc<BTreeSet<String>>) -> u32 {
    u32::try_from(set.len()).unwrap_or(u32::MAX).max(1)
}

impl Fleets {
    /// Expires one cached workspace enumeration for a live-store fixture.
    ///
    /// Production writers invalidate this cache themselves. A fixture that
    /// inserts a row directly needs the same hook so it can prove the refresh
    /// without sleeping out the production TTL.
    #[cfg(feature = "test-util")]
    pub async fn invalidate_live_set(&self, workspace: &Uuid7) {
        self.live_sets.invalidate(workspace.as_str()).await;
    }

    /// Every fleet this workspace holds, by identifier.
    ///
    /// Sorted, because the set is announced to a client as a list and a set
    /// whose order changed between ticks would read as a set that changed.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, to every reader that waited
    /// on the same load. A workspace holding no fleets is an empty set, not a
    /// failure — a wall with nothing on it is a
    /// state an operator reaches by deleting their last fleet.
    pub async fn live_set(&self, workspace: &Uuid7) -> Result<Arc<BTreeSet<String>>> {
        let load = async { self.enumerate(workspace).await.map(Arc::new) };
        self.live_sets
            .try_get_with_by_ref(workspace.as_str(), load)
            .await
            .map_err(error::shared)
    }

    /// The statement behind [`Self::live_set`], run on a miss.
    async fn enumerate(&self, workspace: &Uuid7) -> Result<BTreeSet<String>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_scalar::<_, String>(sql::SELECT_FLEET_IDS)
            .bind(workspace.as_str())
            .fetch_all(connection.as_mut())
            .await
            .map(|ids| ids.into_iter().collect())
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
        let mut connection = self.database.acquire().await?;
        sqlx::query_as::<_, CounterRow>(sql::SELECT_FLEET_COUNTERS_FOR_SET)
            .bind(workspace.as_str())
            .bind(fleets)
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
    id: String,
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
        (self.id, counters)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use super::{LIVE_SET_FLEETS, live_sets, weight};

    fn set(fleets: u64) -> Arc<BTreeSet<String>> {
        Arc::new((0..fleets).map(|index| format!("fleet-{index}")).collect())
    }

    #[test]
    fn test_a_set_weighs_its_fleets_and_an_empty_one_weighs_one() {
        let key = String::from("workspace");
        assert_eq!(weight(&key, &set(0)), 1);
        assert_eq!(weight(&key, &set(3)), 3);
    }

    #[tokio::test]
    async fn test_large_workspace_weighs_more() {
        let cache = live_sets();
        for small in ["a", "b", "c"] {
            cache.insert(small.to_owned(), set(2)).await;
        }
        cache.run_pending_tasks().await;
        // One workspace holding more fleets than the whole bound.
        cache
            .insert("large".to_owned(), set(LIVE_SET_FLEETS + 1))
            .await;
        cache.run_pending_tasks().await;
        assert!(
            cache.get("large").await.is_none(),
            "the large set goes by its weight"
        );
        for small in ["a", "b", "c"] {
            assert!(cache.get(small).await.is_some(), "{small} keeps its entry");
        }
        assert!(cache.weighted_size() <= LIVE_SET_FLEETS);
    }
}
