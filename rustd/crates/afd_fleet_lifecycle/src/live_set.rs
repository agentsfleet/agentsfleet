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
//! # What this cache does NOT yet do, stated because the difference is
//! measurable
//!
//! It does not COALESCE concurrent misses. `get` followed by `insert` is
//! check-then-act: when an entry expires under V viewers of one workspace, all
//! V miss and all V run the statement, and one of them wins the `insert`. So
//! the saving here is the steady state — V viewers ticking every few seconds
//! cost one statement per TTL instead of V per tick — and not the expiry
//! instant, which still costs V.
//!
//! Closing that needs `moka`'s `try_get_with`, whose init is shared across
//! waiters and therefore hands every waiter the SAME failure behind an `Arc`.
//! `std` implements `Error` for neither `Arc<T>` nor anything this crate's
//! error shell can lift, so the honest version of that change alters this
//! crate's error model — a caller today gets `INTERNAL_DB_UNAVAILABLE` or
//! `INTERNAL_DB_QUERY` off the real failure, and a shared error would have to
//! keep telling them apart. That is a deliberate follow-up rather than
//! something to smuggle in behind a stream milestone, and the comment says so
//! rather than claiming a coalescing the code does not perform.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use afd_core::id::Uuid7;
use moka::future::Cache;
use sqlx::Row as _;

use crate::Fleets;
use crate::error::{self, Result};
use crate::sql;

/// What a failure in this read is reported as having been doing.
const CONTEXT_LIVE_SET: &str = "enumerate a workspace's fleets";

/// How long an enumeration is served before it is read again.
///
/// The cadence a stream's refresh tick runs on, so a fleet installed while a
/// wall is open appears within one beat of it. Short enough that an operator
/// does not notice, long enough that a hundred tabs are one statement.
pub(crate) const LIVE_SET_TTL: Duration = Duration::from_secs(10);

/// How many workspaces' sets are held at once.
///
/// A bound rather than a promise: an entry is a workspace id and a handful of
/// fleet ids, and evicting one costs the next tick a query.
pub(crate) const LIVE_SET_CAPACITY: u64 = 4_096;

/// The cache the enumeration is served from.
pub(crate) type LiveSets = Cache<String, Arc<BTreeSet<String>>>;

/// A cache sized and aged for the stream refresh tick.
pub(crate) fn live_sets() -> LiveSets {
    Cache::builder()
        .max_capacity(LIVE_SET_CAPACITY)
        .time_to_live(LIVE_SET_TTL)
        .build()
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
    /// Reports a datastore that would not answer. A workspace holding no
    /// fleets is an empty set, not a failure — a wall with nothing on it is a
    /// state an operator reaches by deleting their last fleet.
    pub async fn live_set(&self, workspace: &Uuid7) -> Result<Arc<BTreeSet<String>>> {
        if let Some(held) = self.live_sets.get(workspace.as_str()).await {
            return Ok(held);
        }
        let enumerated = Arc::new(self.enumerate(workspace).await?);
        self.live_sets
            .insert(workspace.as_str().to_owned(), Arc::clone(&enumerated))
            .await;
        Ok(enumerated)
    }

    /// The statement behind [`Self::live_set`], run on a miss.
    async fn enumerate(&self, workspace: &Uuid7) -> Result<BTreeSet<String>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::SELECT_FLEET_IDS)
            .bind(workspace.as_str())
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_LIVE_SET))?;
        rows.iter()
            .map(|row| {
                row.try_get::<String, _>(0)
                    .map_err(error::query(CONTEXT_LIVE_SET))
            })
            .collect()
    }
}
