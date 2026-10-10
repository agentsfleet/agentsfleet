//! The lease a runner verb names, proved before the verb touches its fleet.
//!
//! The schedules and messages verbs act on a fleet the request never names:
//! the runner names its LEASE, and the fleet, the workspace and the event are
//! read from the lease row. A body therefore cannot reach another fleet's
//! schedules or another event's thread, and a holder a reclaim has superseded
//! is refused before anything is read or written on its behalf.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use crate::error::{Result, lease_not_found, query, row_malformed, stale_fence};
use crate::lease::pull::Plane;
use crate::lease::sql;
use crate::lease::sql::standing::SELECT_STANDING;
use crate::lease::store::Leases;

/// Statement name, for the context a query failure carries.
const CONTEXT_STANDING: &str = "lease standing lookup";

/// The table a malformed column is reported against.
const TABLE: &str = "fleet.runner_leases";

/// The columns a malformed value is reported under.
const COLUMN_FLEET: &str = "fleet_id";
/// See [`COLUMN_FLEET`].
const COLUMN_WORKSPACE: &str = "workspace_id";

/// A superseded holder reached a lease verb, and nothing was done for it.
const EVENT_FENCED: &str = "lease_verb_fenced";

/// A lease this runner holds live, and the fleet's current holder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// The lease itself.
    pub lease_id: Uuid7,
    /// The fleet it runs.
    pub fleet_id: Uuid7,
    /// The workspace that fleet belongs to.
    pub workspace_id: Uuid7,
    /// The event it is running, as the ledger names it.
    pub event_id: String,
    /// Who woke the fleet for that event, as the ledger recorded it: a
    /// schedule's fire reads `cron:<schedule_id>`.
    pub actor: String,
}

/// A lease row as the `live_lease!` statements read it, before the fence
/// is applied: what the standing proof and the tool-call records both read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveLease {
    /// The fleet the lease runs.
    pub(crate) fleet_id: String,
    /// The workspace that fleet belongs to.
    pub(crate) workspace_id: String,
    /// The event the lease runs.
    pub(crate) event_id: String,
    /// Who woke the fleet for that event.
    pub(crate) actor: String,
    /// The lease's own fencing token, which keys every row it writes.
    pub(crate) fence: i64,
    /// The fleet's live sequence.
    pub(crate) live_seq: i64,
}

impl LiveLease {
    /// The row a `live_lease!` statement answered, in its column order.
    pub(crate) fn read(row: &PgRow, context: &'static str) -> Result<Self> {
        Ok(Self {
            fleet_id: row.try_get(0).map_err(query(context))?,
            workspace_id: row.try_get(1).map_err(query(context))?,
            event_id: row.try_get(2).map_err(query(context))?,
            fence: row.try_get(3).map_err(query(context))?,
            live_seq: row.try_get(4).map_err(query(context))?,
            actor: row.try_get(5).map_err(query(context))?,
        })
    }

    /// Whether this lease still holds the fleet, and `presented` is its own
    /// token.
    ///
    /// The one fence rule every lease-addressed verb applies: the tool-call
    /// records read it under a row lock, the schedules and messages verbs
    /// without.
    pub(crate) fn holds(&self, presented: u64) -> bool {
        fence_holds(self.fence, self.live_seq, presented)
    }
}

/// The fence rule itself: the lease's own token is the fleet's live one, and
/// it is the token `presented`.
pub(crate) fn fence_holds(fence: i64, live_seq: i64, presented: u64) -> bool {
    fence_current(fence, live_seq) && u64::try_from(fence).is_ok_and(|own| own == presented)
}

/// Its first half, for a verb that presents no token (a hydrate): no reclaim
/// has moved the fleet past the lease whose own token is `fence`.
pub(crate) const fn fence_current(fence: i64, live_seq: i64) -> bool {
    fence >= live_seq
}

impl Plane {
    /// Proves `lease_id` is `runner_id`'s live lease, and the fleet's current
    /// holder under the presented token.
    ///
    /// Takes the lease by value: the proved standing carries it on, so the
    /// verb that asked does not hold a second copy.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's or not live, and a holder the
    /// fleet has superseded. Reports a datastore that would not answer, and a
    /// stored identifier this daemon cannot read.
    pub async fn standing(
        &self,
        runner_id: &Uuid7,
        lease_id: Uuid7,
        presented: u64,
        now: UnixMillis,
    ) -> Result<Standing> {
        let Some(read) = self.leases.standing_row(runner_id, &lease_id, now).await? else {
            return Err(lease_not_found());
        };
        if !read.holds(presented) {
            let fleet_id = read.fleet_id.as_str();
            let live_seq = read.live_seq;
            tracing::debug!(
                fleet_id,
                fencing_token = presented,
                live_seq,
                event = EVENT_FENCED
            );
            return Err(stale_fence());
        }
        Ok(Standing {
            lease_id,
            fleet_id: Uuid7::parse(&read.fleet_id).map_err(row_malformed(TABLE, COLUMN_FLEET))?,
            workspace_id: Uuid7::parse(&read.workspace_id)
                .map_err(row_malformed(TABLE, COLUMN_WORKSPACE))?,
            event_id: read.event_id,
            actor: read.actor,
        })
    }
}

impl Leases {
    /// The lease `lease_id`, if `runner_id` holds it live.
    async fn standing_row(
        &self,
        runner_id: &Uuid7,
        lease_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Option<LiveLease>> {
        let mut connection = self.pool().acquire().await?;
        let found = sqlx::query(SELECT_STANDING)
            .bind(lease_id.as_str())
            .bind(runner_id.as_str())
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(now.as_millis())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_STANDING))?;
        found
            .map(|row| LiveLease::read(&row, CONTEXT_STANDING))
            .transpose()
    }
}

#[cfg(test)]
#[path = "standing/tests.rs"]
mod tests;
