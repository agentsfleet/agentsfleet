//! The run a runner verb names, proved before the verb touches its fleet.
//!
//! The schedules and messages verbs act on a fleet the request never names:
//! the runner names its RUN, and the fleet, the workspace and the event are
//! read from the run row. A body therefore cannot reach another fleet's
//! schedules or another event's thread, and a holder a reclaim has superseded
//! is refused before anything is read or written on its behalf.

use af_core::id::Uuid7;

use crate::error::{Result, query, run_not_found, stale_fence};
use crate::run::pull::RunService;
use crate::run::sql;
use crate::run::sql::standing::SELECT_STANDING;
use crate::run::store::Runs;

/// Statement name, for the context a query failure carries.
const CONTEXT_STANDING: &str = "run standing lookup";

/// A superseded holder reached a run verb, and nothing was done for it.
const EVENT_FENCED: &str = "run_verb_fenced";

/// A run this runner holds live, and the fleet's current holder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// The run itself.
    pub run_id: Uuid7,
    /// The fleet it runs.
    pub fleet_id: Uuid7,
    /// The workspace that fleet belongs to.
    pub workspace_id: Uuid7,
    /// The event it is running, as the ledger names it.
    pub turn_id: Uuid7,
    /// Who woke the fleet for that event, as the ledger recorded it: a
    /// schedule's fire reads `cron:<schedule_id>`.
    pub actor: String,
}

/// A run row as the `live_run.sql` statements read it, before the fence
/// is applied: what the standing proof and the tool-call records both read.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct LiveRun {
    /// The fleet the run runs.
    pub(crate) fleet_id: Uuid7,
    /// The workspace that fleet belongs to.
    pub(crate) workspace_id: Uuid7,
    /// The event the run runs.
    pub(crate) turn_id: Uuid7,
    /// Who woke the fleet for that event.
    pub(crate) actor: String,
    /// The run's own fencing token, which keys every row it writes.
    pub(crate) fence: i64,
    /// The fleet's live sequence.
    pub(crate) live_seq: i64,
}

impl LiveRun {
    /// Whether this run still holds the fleet, and `presented` is its own
    /// token.
    ///
    /// The one fence rule every run-addressed verb applies: the tool-call
    /// records read it under a row lock, the schedules and messages verbs
    /// without.
    pub(crate) fn holds(&self, presented: u64) -> bool {
        fence_holds(self.fence, self.live_seq, presented)
    }
}

/// The fence rule itself: the run's own token is the fleet's live one, and
/// it is the token `presented`.
pub(crate) fn fence_holds(fence: i64, live_seq: i64, presented: u64) -> bool {
    fence_current(fence, live_seq) && u64::try_from(fence).is_ok_and(|own| own == presented)
}

/// Its first half, for a verb that presents no token (a hydrate): no reclaim
/// has moved the fleet past the run whose own token is `fence`.
pub(crate) const fn fence_current(fence: i64, live_seq: i64) -> bool {
    fence >= live_seq
}

impl RunService {
    /// Proves `run_id` is `runner_id`'s live run, and the fleet's current
    /// holder under the presented token.
    ///
    /// Takes the run by value: the proved standing carries it on, so the
    /// verb that asked does not hold a second copy.
    ///
    /// # Errors
    /// Refuses a run that is not this runner's or not live, and a holder the
    /// fleet has superseded. Reports a datastore that would not answer, and a
    /// stored identifier this daemon cannot read.
    pub async fn standing(
        &self,
        runner_id: &Uuid7,
        run_id: Uuid7,
        presented: u64,
    ) -> Result<Standing> {
        let Some(read) = self.runs.standing_row(runner_id, &run_id).await? else {
            return Err(run_not_found());
        };
        if !read.holds(presented) {
            let fleet_id = read.fleet_id.as_str();
            let live_seq = read.live_seq;
            {
                let (fencing_token, event) = (&presented, &EVENT_FENCED);
                tracing::debug!(fleet_id, fencing_token, live_seq, event);
            };
            return Err(stale_fence());
        }
        Ok(Standing {
            run_id,
            fleet_id: read.fleet_id.clone(),
            workspace_id: read.workspace_id.clone(),
            turn_id: read.turn_id,
            actor: read.actor,
        })
    }
}

impl Runs {
    /// The run `run_id`, if `runner_id` holds it live.
    async fn standing_row(&self, runner_id: &Uuid7, run_id: &Uuid7) -> Result<Option<LiveRun>> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query_as(SELECT_STANDING)
            .bind(run_id)
            .bind(runner_id)
            .bind(sql::RUN_STATUS_ACTIVE)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_STANDING))
    }
}

#[cfg(test)]
mod tests;
